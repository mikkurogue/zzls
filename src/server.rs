use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use tower_lsp_server::jsonrpc::Result;
use tower_lsp_server::ls_types::*;
use tower_lsp_server::ls_types::request::{GotoDeclarationParams, GotoDeclarationResponse};
use tower_lsp_server::{Client, LanguageServer};

use crate::bridge::{ZigCompiler, ZigFormatter};
use crate::config::Config;
use crate::document::Document;
use crate::features;
use crate::workspace::Workspace;

#[allow(dead_code)]
pub struct Backend {
    client: Client,
    config: Arc<tokio::sync::RwLock<Config>>,
    workspace: Arc<tokio::sync::RwLock<Option<Workspace>>>,
    documents: Arc<tokio::sync::RwLock<HashMap<Uri, Document>>>,
    compiler: Arc<tokio::sync::RwLock<Option<ZigCompiler>>>,
    formatter: Arc<tokio::sync::RwLock<Option<ZigFormatter>>>,
    std_modules: Arc<tokio::sync::RwLock<Vec<String>>>,
}

impl Backend {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            config: Arc::new(tokio::sync::RwLock::new(Config::default())),
            workspace: Arc::new(tokio::sync::RwLock::new(None)),
            documents: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            compiler: Arc::new(tokio::sync::RwLock::new(None)),
            formatter: Arc::new(tokio::sync::RwLock::new(None)),
            std_modules: Arc::new(tokio::sync::RwLock::new(Vec::new())),
        }
    }

    fn get_file_path(uri: &Uri) -> Option<PathBuf> {
        uri.to_file_path().map(|p| p.into_owned())
    }

    async fn ensure_compiler(&self) -> Option<ZigCompiler> {
        let compiler = self.compiler.read().await;
        if let Some(c) = compiler.as_ref() {
            return Some(c.clone());
        }
        drop(compiler);

        let zig_path = self.config.read().await.zig_path.clone()
            .or_else(|| which::which("zig").ok());

        if let Some(zig) = zig_path {
            let compiler = ZigCompiler::new(zig);
            *self.compiler.write().await = Some(compiler.clone());
            Some(compiler)
        } else {
            self.client
                .log_message(MessageType::WARNING, "zig not found in PATH")
                .await;
            None
        }
    }
}

impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        tracing::info!("Initializing ZZLS");

        #[allow(deprecated)]
        let workspace_root = params.root_uri.as_ref()
            .and_then(|uri| Self::get_file_path(uri));

        if let Some(ref root) = workspace_root {
            tracing::info!("Workspace root: {}", root.display());
            *self.workspace.write().await = Some(Workspace::new(root.clone()));
        }

        if workspace_root.is_some() {
            self.ensure_compiler().await;
        }

        // Load std library module names for completions
        let std_mods = load_std_modules();
        *self.std_modules.write().await = std_mods;

        let capabilities = ServerCapabilities {
            text_document_sync: Some(TextDocumentSyncCapability::Options(
                TextDocumentSyncOptions {
                    open_close: Some(true),
                    change: Some(TextDocumentSyncKind::FULL),
                    save: Some(TextDocumentSyncSaveOptions::SaveOptions(SaveOptions {
                        include_text: Some(true),
                    })),
                    ..Default::default()
                },
            )),
            hover_provider: Some(HoverProviderCapability::Simple(true)),
            completion_provider: Some(CompletionOptions {
                trigger_characters: Some(vec![".".to_string(), "@".to_string(), "\"".to_string()]),
                ..Default::default()
            }),
            definition_provider: Some(OneOf::Left(true)),
            declaration_provider: Some(DeclarationCapability::Simple(true)),
            document_symbol_provider: Some(OneOf::Left(true)),
            references_provider: Some(OneOf::Left(true)),
            rename_provider: Some(OneOf::Left(true)),
            code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
            execute_command_provider: Some(ExecuteCommandOptions {
                commands: vec![],
                ..Default::default()
            }),
            ..Default::default()
        };

        let server_info = ServerInfo {
            name: "zzls".to_string(),
            version: Some(env!("CARGO_PKG_VERSION").to_string()),
        };

        Ok(InitializeResult {
            capabilities,
            server_info: Some(server_info),
            offset_encoding: None,
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        tracing::info!("ZZLS initialized");

        self.client
            .log_message(MessageType::INFO, "ZZLS initialized successfully")
            .await;

        self.client
            .register_capability(vec![Registration {
                id: "config-change".to_string(),
                method: "workspace/didChangeConfiguration".to_string(),
                register_options: Some(
                    serde_json::json!({ "section": "zzls" }),
                ),
            }])
            .await
            .ok();
    }

    async fn shutdown(&self) -> Result<()> {
        tracing::info!("ZZLS shutting down");
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        let text = params.text_document.text;

        tracing::debug!("Document opened: {}", uri.as_str());

        let document = Document::new(
            uri.clone(),
            text,
            params.text_document.version,
        );

        self.documents.write().await.insert(uri.clone(), document);
        self.publish_diagnostics(&uri).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri.clone();

        tracing::debug!("Document changed: {}", uri.as_str());

        if let Some(doc) = self.documents.write().await.get_mut(&uri) {
            for change in params.content_changes {
                doc.update(&change.text, params.text_document.version);
            }
        }
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        let uri = params.text_document.uri.clone();

        tracing::info!("Document saved: {}", uri.as_str());

        self.publish_diagnostics(&uri).await;

        if self.config.read().await.format_on_save {
            self.format_document(&uri).await.ok();
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri.clone();

        tracing::debug!("Document closed: {}", uri.as_str());

        self.documents.write().await.remove(&uri);

        self.client
            .publish_diagnostics(uri, vec![], None)
            .await;
    }

    async fn formatting(&self, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        let uri = params.text_document.uri;

        tracing::info!("Formatting requested for: {}", uri.as_str());

        let document = self.documents.read().await;
        let doc = document.get(&uri).ok_or_else(|| {
            tower_lsp_server::jsonrpc::Error::invalid_params("Document not found")
        })?;

        let source = doc.get_text();

        let zig_path = self.config.read().await.zig_path.clone()
            .or_else(|| which::which("zig").ok())
            .ok_or_else(|| tower_lsp_server::jsonrpc::Error::internal_error())?;

        let formatter = ZigFormatter::new(zig_path);

        match formatter.format_source(&source).await {
            Ok(formatted) => {
                if formatted == source {
                    Ok(None)
                } else {
                    let line_count = source.lines().count();
                    let last_line = source.lines().last().map(|l| l.len()).unwrap_or(0);

                    Ok(Some(vec![TextEdit {
                        range: Range {
                            start: Position::new(0, 0),
                            end: Position::new(line_count as u32, last_line as u32),
                        },
                        new_text: formatted,
                    }]))
                }
            }
            Err(e) => {
                tracing::error!("Formatting failed: {}", e);
                Err(tower_lsp_server::jsonrpc::Error::internal_error())
            }
        }
    }

    async fn range_formatting(&self, params: DocumentRangeFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        let uri = params.text_document.uri;

        tracing::info!("Range formatting requested for: {}", uri.as_str());

        let document = self.documents.read().await;
        let doc = document.get(&uri).ok_or_else(|| {
            tower_lsp_server::jsonrpc::Error::invalid_params("Document not found")
        })?;

        let source = doc.get_text();

        let zig_path = self.config.read().await.zig_path.clone()
            .or_else(|| which::which("zig").ok())
            .ok_or_else(|| tower_lsp_server::jsonrpc::Error::internal_error())?;

        let formatter = ZigFormatter::new(zig_path);

        match formatter.format_source(&source).await {
            Ok(formatted) => {
                let line_count = source.lines().count();
                let last_line = source.lines().last().map(|l| l.len()).unwrap_or(0);

                Ok(Some(vec![TextEdit {
                    range: Range {
                        start: Position::new(0, 0),
                        end: Position::new(line_count as u32, last_line as u32),
                    },
                    new_text: formatted,
                }]))
            }
            Err(e) => {
                tracing::error!("Range formatting failed: {}", e);
                Err(tower_lsp_server::jsonrpc::Error::internal_error())
            }
        }
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        tracing::debug!("Hover requested at {}:{}", uri.as_str(), position.line);

        Ok(None)
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = &params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;

        tracing::debug!("Completion requested at {}:{}", uri.as_str(), position.line);

        let document = self.documents.read().await;
        let doc = match document.get(uri) {
            Some(d) => d,
            None => return Ok(Some(CompletionResponse::Array(vec![]))),
        };

        let workspace_root = self.workspace.read().await.as_ref().map(|w| w.root().to_path_buf());

        // Cache std modules
        let std_modules = self.std_modules.read().await.clone();

        let ctx = features::CompletionContext {
            document: doc,
            position,
            workspace_root: workspace_root.as_deref(),
            std_modules: &std_modules,
        };

        let items = features::provide_completions(ctx);

        Ok(Some(CompletionResponse::Array(items)))
    }

    async fn goto_definition(&self, params: GotoDefinitionParams) -> Result<Option<GotoDefinitionResponse>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        tracing::debug!("Goto definition requested at {}:{}", uri.as_str(), position.line);

        Ok(None)
    }

    async fn goto_declaration(&self, params: GotoDeclarationParams) -> Result<Option<GotoDeclarationResponse>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        tracing::debug!("Goto declaration requested at {}:{}", uri.as_str(), position.line);

        Ok(None)
    }

    async fn document_symbol(&self, params: DocumentSymbolParams) -> Result<Option<DocumentSymbolResponse>> {
        let uri = params.text_document.uri;

        tracing::debug!("Document symbols requested for: {}", uri.as_str());

        let document = self.documents.read().await;
        let doc = document.get(&uri).ok_or_else(|| {
            tower_lsp_server::jsonrpc::Error::invalid_params("Document not found")
        })?;

        let symbols = doc.extract_symbols();
        Ok(Some(DocumentSymbolResponse::Nested(symbols)))
    }

    async fn symbol(&self, params: WorkspaceSymbolParams) -> Result<Option<WorkspaceSymbolResponse>> {
        tracing::debug!("Workspace symbols requested for: {}", params.query);

        Ok(None)
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let uri = &params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;

        tracing::debug!("References requested at {}:{}", uri.as_str(), position.line);

        Ok(None)
    }

    async fn rename(&self, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
        let uri = &params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        let new_name = &params.new_name;

        tracing::debug!("Rename requested at {}:{} to '{}'", uri.as_str(), position.line, new_name);

        Ok(None)
    }

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let uri = &params.text_document.uri;

        tracing::debug!("Code action requested for: {}", uri.as_str());

        Ok(None)
    }

    async fn execute_command(&self, params: ExecuteCommandParams) -> Result<Option<LSPAny>> {
        tracing::debug!("Execute command: {}", params.command);

        Ok(None)
    }
}

impl Backend {
    async fn publish_diagnostics(&self, uri: &Uri) {
        let document = self.documents.read().await;
        let doc = match document.get(uri) {
            Some(d) => d,
            None => return,
        };

        let source = doc.get_text();
        let path = match Self::get_file_path(uri) {
            Some(p) => p,
            None => return,
        };

        let compiler = self.compiler.read().await;
        let diagnostics = if let Some(ref c) = *compiler {
            match c.check_source(&source, &path).await {
                Ok(d) => d,
                Err(e) => {
                    tracing::error!("Failed to run compiler: {}", e);
                    vec![]
                }
            }
        } else {
            vec![]
        };

        let lsp_diagnostics: Vec<Diagnostic> = diagnostics
            .into_iter()
            .map(|d| crate::diagnostics::zig_to_lsp_diagnostic(d))
            .collect();

        self.client
            .publish_diagnostics(uri.clone(), lsp_diagnostics, None)
            .await;
    }

    async fn format_document(&self, uri: &Uri) -> Result<()> {
        let document = self.documents.read().await;
        let doc = match document.get(uri) {
            Some(d) => d,
            None => return Ok(()),
        };

        let source = doc.get_text();

        let zig_path = self.config.read().await.zig_path.clone()
            .or_else(|| which::which("zig").ok())
            .ok_or_else(|| tower_lsp_server::jsonrpc::Error::internal_error())?;

        let formatter = ZigFormatter::new(zig_path);

        match formatter.format_source(&source).await {
            Ok(formatted) if formatted != source => {
                let line_count = source.lines().count();
                let last_line = source.lines().last().map(|l| l.len()).unwrap_or(0);

                let edits = vec![TextEdit {
                    range: Range {
                        start: Position::new(0, 0),
                        end: Position::new(line_count as u32, last_line as u32),
                    },
                    new_text: formatted,
                }];

                let workspace_edit = WorkspaceEdit {
                    changes: Some(HashMap::from([(uri.clone(), edits)])),
                    ..Default::default()
                };

                self.client
                    .apply_edit(workspace_edit)
                    .await?;
            }
            _ => {}
        }

        Ok(())
    }
}

fn load_std_modules() -> Vec<String> {
    let output = match std::process::Command::new("zig").arg("env").output() {
        Ok(o) => o,
        Err(_) => return Vec::new(),
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let std_dir = stdout.lines()
        .find_map(|line| {
            let trimmed = line.trim();
            if trimmed.starts_with(".std_dir") {
                if let Some(start) = trimmed.find('"') {
                    if let Some(end) = trimmed.rfind('"') {
                        if start != end {
                            return Some(PathBuf::from(&trimmed[start + 1..end]));
                        }
                    }
                }
            }
            None
        });

    let std_dir = match std_dir {
        Some(d) => d,
        None => return Vec::new(),
    };

    let mut modules = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&std_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().map_or(false, |e| e == "zig") {
                if let Some(name) = path.file_stem() {
                    modules.push(name.to_string_lossy().to_string());
                }
            }
        }
    }

    modules.sort();
    tracing::info!("Loaded {} std library modules", modules.len());
    modules
}
