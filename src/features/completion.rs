use std::path::{Path, PathBuf};

use tower_lsp_server::ls_types::*;

use crate::document::Document;

pub struct CompletionContext<'a> {
    pub document: &'a Document,
    pub position: Position,
    pub workspace_root: Option<&'a Path>,
    pub std_modules: &'a [String],
}

/// Provide completions based on context
pub fn provide_completions(ctx: CompletionContext) -> Vec<CompletionItem> {
    let text = ctx.document.get_text();
    let offset = ctx.document.position_to_offset(ctx.position);
    let line = ctx.document.get_line(ctx.position.line as usize);

    // Determine what kind of completion the user wants
    let trigger = get_trigger_context(&line, ctx.position.character as usize);

    match trigger {
        TriggerContext::DottedAccess(prefix) => {
            // User typed "something." - suggest members
            provide_dot_completions(&text, offset, &prefix, ctx)
        }
        TriggerContext::ImportPrefix => {
            // User typed @import(" - suggest files
            provide_import_completions(ctx)
        }
        TriggerContext::StdAccess => {
            // User typed "std." - suggest std modules
            provide_std_completions(ctx)
        }
        TriggerContext::Root => {
            // Top-level - suggest keywords + declarations
            provide_root_completions(ctx)
        }
    }
}

enum TriggerContext {
    /// `prefix.` - suggest members of prefix
    DottedAccess(String),
    /// `@import("` - suggest files
    ImportPrefix,
    /// `std.` - suggest std modules
    StdAccess,
    /// Top-level / no prefix
    Root,
}

fn get_trigger_context(line: &str, col: usize) -> TriggerContext {
    let before = &line[..col.min(line.len())];

    // Check for @import(" prefix
    if let Some(pos) = before.rfind("@import(\"") {
        // Make sure this is the last @import and the quote is unclosed
        let after_import = &before[pos + 9..];
        if after_import.matches('"').count() % 2 == 0 {
            return TriggerContext::ImportPrefix;
        }
    }

    // Find the last dot and the identifier before it
    if let Some(dot_pos) = before.rfind('.') {
        let before_dot = before[..dot_pos].trim_end();
        if let Some(ident) = extract_last_ident(before_dot) {
            if ident == "std" {
                return TriggerContext::StdAccess;
            }
            return TriggerContext::DottedAccess(ident);
        }
    }

    TriggerContext::Root
}

fn extract_last_ident(s: &str) -> Option<String> {
    let trimmed = s.trim_end();
    if trimmed.is_empty() {
        return None;
    }

    let end = trimmed.len();
    let mut start = end;

    for ch in trimmed.chars().rev() {
        if ch.is_alphanumeric() || ch == '_' {
            start -= ch.len_utf8();
        } else {
            break;
        }
    }

    if start == end {
        return None;
    }

    let ident = &trimmed[start..end];
    if ident.is_empty() || ident.chars().next().map_or(false, |c| c.is_ascii_digit()) {
        return None;
    }

    Some(ident.to_string())
}

fn provide_dot_completions(
    text: &str,
    _offset: usize,
    prefix: &str,
    ctx: CompletionContext,
) -> Vec<CompletionItem> {
    // Try to find what `prefix` is - check declarations in the file
    let decls = extract_declarations(text);

    // Check if prefix is a known module or variable
    if prefix == "std" {
        return provide_std_completions(ctx);
    }

    // Check if prefix matches a known declaration and provide its members
    if let Some(decl) = decls.iter().find(|d| d.name == prefix) {
        return provide_type_members(decl, ctx);
    }

    // Check if prefix matches a std module
    if ctx.std_modules.iter().any(|m| m == prefix) {
        // It's a std submodule - suggest its public members
        return provide_std_submodule_members(prefix, ctx);
    }

    vec![]
}

fn provide_std_completions(ctx: CompletionContext) -> Vec<CompletionItem> {
    let mut items: Vec<CompletionItem> = ctx
        .std_modules
        .iter()
        .map(|name| CompletionItem {
            label: name.clone(),
            kind: Some(CompletionItemKind::MODULE),
            detail: Some("std module".to_string()),
            documentation: Some(Documentation::String(format!("std.{name}"))),
            ..Default::default()
        })
        .collect();

    // Also add common std types/functions as top-level completions
    let common = vec![
        ("debug", "std.debug"),
        ("mem", "std.mem"),
        ("fmt", "std.fmt"),
        ("math", "std.math"),
        ("fs", "std.fs"),
        ("io", "std.io"),
        ("json", "std.json"),
        ("heap", "std.heap"),
        ("crypto", "std.crypto"),
        ("base64", "std.base64"),
        ("ascii", "std.ascii"),
        ("log", "std.log"),
        ("os", "std.os"),
        ("meta", "std.meta"),
        ("testing", "std.testing"),
        ("builtin", "std.builtin"),
        ("ArrayList", "std.ArrayList"),
        ("HashMap", "std.HashMap"),
        ("StringHashMap", "std.StringHashMap"),
        ("ArrayHashMap", "std.ArrayHashMap"),
        ("StringArrayHashMap", "std.StringArrayHashMap"),
    ];

    for (name, detail) in common {
        if !items.iter().any(|i| i.label == name) {
            items.push(CompletionItem {
                label: name.to_string(),
                kind: Some(CompletionItemKind::MODULE),
                detail: Some(detail.to_string()),
                ..Default::default()
            });
        }
    }

    items
}

fn provide_std_submodule_members(module: &str, ctx: CompletionContext) -> Vec<CompletionItem> {
    let std_dir = get_std_dir(ctx.workspace_root);
    if let Some(std_dir) = std_dir {
        let module_path = std_dir.join(format!("{module}.zig"));
        if let Ok(content) = std::fs::read_to_string(&module_path) {
            return extract_pub_declarations(&content)
                .into_iter()
                .map(|decl| CompletionItem {
                    label: decl.name.clone(),
                    kind: Some(match decl.kind.as_str() {
                        "fn" => CompletionItemKind::FUNCTION,
                        "const" | "var" => CompletionItemKind::CONSTANT,
                        "struct" => CompletionItemKind::STRUCT,
                        "enum" => CompletionItemKind::ENUM,
                        "union" => CompletionItemKind::ENUM,
                        "type" | "interface" => CompletionItemKind::INTERFACE,
                        _ => CompletionItemKind::VARIABLE,
                    }),
                    detail: Some(format!("{module}.{}", decl.name)),
                    ..Default::default()
                })
                .collect();
        }
    }

    vec![]
}

fn provide_type_members(decl: &Declaration, _ctx: CompletionContext) -> Vec<CompletionItem> {
    // Provide basic members based on type kind
    match decl.kind.as_str() {
        "struct" => {
            vec![
                CompletionItem::new_simple("init".to_string(), "Constructor".to_string()),
                CompletionItem::new_simple("deinit".to_string(), "Destructor".to_string()),
            ]
        }
        "enum" => {
            vec![
                CompletionItem::new_simple("fromInt".to_string(), "Create from integer".to_string()),
                CompletionItem::new_simple("toInt".to_string(), "Convert to integer".to_string()),
            ]
        }
        _ => vec![],
    }
}

fn provide_import_completions(ctx: CompletionContext) -> Vec<CompletionItem> {
    let mut items = Vec::new();

    if let Some(workspace) = ctx.workspace_root {
        // Find .zig files in src/
        let src_dir = workspace.join("src");
        if src_dir.exists() {
            collect_zig_files(&src_dir, workspace, &mut items);
        }

        // Also suggest std modules
        for module in ctx.std_modules {
            items.push(CompletionItem {
                label: module.clone(),
                kind: Some(CompletionItemKind::FILE),
                detail: Some(format!("std.{module}")),
                insert_text: Some(format!("{module}\"")),
                ..Default::default()
            });
        }
    }

    items
}

fn collect_zig_files(dir: &Path, base: &Path, items: &mut Vec<CompletionItem>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let name = path.file_name().unwrap_or_default().to_string_lossy();
                if !["zig-cache", "zig-out", ".git", "node_modules", ".zig-cache"].contains(&name.as_ref()) {
                    collect_zig_files(&path, base, items);
                }
            } else if path.extension().map_or(false, |e| e == "zig") {
                if let Ok(relative) = path.strip_prefix(base) {
                    let label = relative.to_string_lossy().to_string();
                    items.push(CompletionItem {
                        label: label.clone(),
                        kind: Some(CompletionItemKind::FILE),
                        detail: Some(label.clone()),
                        insert_text: Some(format!("{label}\"")),
                        ..Default::default()
                    });
                }
            }
        }
    }
}

fn provide_root_completions(ctx: CompletionContext) -> Vec<CompletionItem> {
    let mut items = Vec::new();

    // Zig keywords
    let keywords = vec![
        ("fn", "Function declaration"),
        ("const", "Constant declaration"),
        ("var", "Variable declaration"),
        ("pub", "Public visibility"),
        ("if", "If expression"),
        ("else", "Else expression"),
        ("while", "While loop"),
        ("for", "For loop"),
        ("return", "Return statement"),
        ("struct", "Struct declaration"),
        ("enum", "Enum declaration"),
        ("union", "Union declaration"),
        ("import", "Import declaration"),
        ("test", "Test declaration"),
        ("defer", "Defer statement"),
        ("errdefer", "Error defer statement"),
        ("try", "Try expression"),
        ("catch", "Catch expression"),
        ("switch", "Switch expression"),
        ("comptime", "Comptime expression"),
        ("inline", "Inline"),
        ("threadlocal", "Thread-local storage"),
        ("break", "Break statement"),
        ("continue", "Continue statement"),
        ("unreachable", "Unreachable"),
        ("undefined", "Undefined value"),
        ("null", "Null value"),
        ("true", "Boolean true"),
        ("false", "Boolean false"),
        ("orelse", "Optional unwrap"),
        ("and", "Boolean and"),
        ("or", "Boolean or"),
        ("anyframe", "Any frame type"),
        ("anytype", "Any type"),
        ("anyerror", "Any error"),
        ("noreturn", "No return type"),
        ("void", "Void type"),
        ("bool", "Boolean type"),
        ("i8", "Signed 8-bit integer"),
        ("i16", "Signed 16-bit integer"),
        ("i32", "Signed 32-bit integer"),
        ("i64", "Signed 64-bit integer"),
        ("i128", "Signed 128-bit integer"),
        ("u8", "Unsigned 8-bit integer"),
        ("u16", "Unsigned 16-bit integer"),
        ("u32", "Unsigned 32-bit integer"),
        ("u64", "Unsigned 64-bit integer"),
        ("u128", "Unsigned 128-bit integer"),
        ("isize", "Signed pointer-sized integer"),
        ("usize", "Unsigned pointer-sized integer"),
        ("f16", "16-bit float"),
        ("f32", "32-bit float"),
        ("f64", "64-bit float"),
        ("f80", "80-bit float"),
        ("f128", "128-bit float"),
        ("type", "Type"),
    ];

    for (kw, detail) in keywords {
        items.push(CompletionItem {
            label: kw.to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            detail: Some(detail.to_string()),
            insert_text: Some(kw.to_string()),
            ..Default::default()
        });
    }

    // Add declarations from current file
    let text = ctx.document.get_text();
    let decls = extract_declarations(&text);

    for decl in &decls {
        items.push(CompletionItem {
            label: decl.name.clone(),
            kind: Some(match decl.kind.as_str() {
                "fn" => CompletionItemKind::FUNCTION,
                "const" => CompletionItemKind::CONSTANT,
                "var" => CompletionItemKind::VARIABLE,
                "struct" => CompletionItemKind::STRUCT,
                "enum" => CompletionItemKind::ENUM,
                "union" => CompletionItemKind::ENUM,
                "test" => CompletionItemKind::FUNCTION,
                _ => CompletionItemKind::VARIABLE,
            }),
            detail: Some(decl.kind.clone()),
            documentation: decl.detail.clone().map(Documentation::String),
            insert_text: Some(decl.name.clone()),
            ..Default::default()
        });
    }

    // Add std as a completion
    items.push(CompletionItem {
        label: "std".to_string(),
        kind: Some(CompletionItemKind::MODULE),
        detail: Some("Standard library".to_string()),
        insert_text: Some("std".to_string()),
        ..Default::default()
    });

    items
}

struct Declaration {
    name: String,
    kind: String,
    detail: Option<String>,
}

fn extract_declarations(text: &str) -> Vec<Declaration> {
    let mut decls = Vec::new();

    for (line_idx, line) in text.lines().enumerate() {
        let trimmed = line.trim();

        // Skip comments and empty lines
        if trimmed.starts_with("//") || trimmed.is_empty() {
            continue;
        }

        // fn declarations
        if let Some(name) = extract_decl_name(trimmed, "fn") {
            let detail = Some(format!("line {}", line_idx + 1));
            decls.push(Declaration {
                name,
                kind: "fn".to_string(),
                detail,
            });
            continue;
        }

        // const declarations (skip @import etc)
        if let Some(name) = extract_decl_name(trimmed, "const") {
            if name.starts_with('@') {
                continue;
            }
            // Try to get the type
            let detail = extract_type_hint(trimmed);
            decls.push(Declaration {
                name,
                kind: "const".to_string(),
                detail,
            });
            continue;
        }

        // var declarations
        if let Some(name) = extract_decl_name(trimmed, "var") {
            let detail = extract_type_hint(trimmed);
            decls.push(Declaration {
                name,
                kind: "var".to_string(),
                detail,
            });
            continue;
        }

        // struct declarations
        if let Some(name) = extract_decl_name(trimmed, "struct") {
            decls.push(Declaration {
                name,
                kind: "struct".to_string(),
                detail: None,
            });
            continue;
        }

        // enum declarations
        if let Some(name) = extract_decl_name(trimmed, "enum") {
            decls.push(Declaration {
                name,
                kind: "enum".to_string(),
                detail: None,
            });
            continue;
        }

        // union declarations
        if let Some(name) = extract_decl_name(trimmed, "union") {
            decls.push(Declaration {
                name,
                kind: "union".to_string(),
                detail: None,
            });
            continue;
        }
    }

    decls
}

fn extract_decl_name(line: &str, keyword: &str) -> Option<String> {
    // Match "keyword name" or "pub keyword name"
    let rest = if line.starts_with(&format!("pub {keyword} ")) {
        line.trim_start_matches("pub ").trim_start()
    } else if line.starts_with(&format!("{keyword} ")) {
        line.trim_start()
    } else {
        return None;
    };

    let after_keyword = rest.strip_prefix(keyword)?.trim_start();
    let name: String = after_keyword
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();

    if name.is_empty() || name.chars().next().map_or(false, |c| c.is_ascii_digit()) {
        return None;
    }

    Some(name)
}

fn extract_type_hint(line: &str) -> Option<String> {
    // Try to extract ": Type" or "= Type" after the name
    if let Some(colon_pos) = line.find(':') {
        let after = &line[colon_pos + 1..].trim_start();
        let type_name: String = after
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '.')
            .collect();
        if !type_name.is_empty() {
            return Some(type_name);
        }
    }
    None
}

fn extract_pub_declarations(content: &str) -> Vec<Declaration> {
    let mut decls = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();

        if !trimmed.starts_with("pub ") {
            continue;
        }

        let after_pub = &trimmed[4..];

        // pub fn
        if let Some(name) = extract_decl_name(after_pub, "fn") {
            decls.push(Declaration {
                name,
                kind: "fn".to_string(),
                detail: None,
            });
            continue;
        }

        // pub const
        if let Some(name) = extract_decl_name(after_pub, "const") {
            decls.push(Declaration {
                name,
                kind: "const".to_string(),
                detail: None,
            });
            continue;
        }

        // pub var
        if let Some(name) = extract_decl_name(after_pub, "var") {
            decls.push(Declaration {
                name,
                kind: "var".to_string(),
                detail: None,
            });
            continue;
        }

        // pub struct
        if let Some(name) = extract_decl_name(after_pub, "struct") {
            decls.push(Declaration {
                name,
                kind: "struct".to_string(),
                detail: None,
            });
            continue;
        }

        // pub enum
        if let Some(name) = extract_decl_name(after_pub, "enum") {
            decls.push(Declaration {
                name,
                kind: "enum".to_string(),
                detail: None,
            });
            continue;
        }

        // pub union
        if let Some(name) = extract_decl_name(after_pub, "union") {
            decls.push(Declaration {
                name,
                kind: "union".to_string(),
                detail: None,
            });
            continue;
        }

        // pub type
        if let Some(name) = extract_decl_name(after_pub, "type") {
            decls.push(Declaration {
                name,
                kind: "type".to_string(),
                detail: None,
            });
            continue;
        }
    }

    decls
}

fn get_std_dir(_workspace_root: Option<&Path>) -> Option<PathBuf> {
    // Try to get std dir from zig env
    let output = std::process::Command::new("zig")
        .arg("env")
        .output()
        .ok()?;

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Parse zig's output format: .std_dir = "/path/to/std"
    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with(".std_dir") {
            // Extract the path between quotes
            if let Some(start) = trimmed.find('"') {
                if let Some(end) = trimmed.rfind('"') {
                    if start != end {
                        return Some(PathBuf::from(&trimmed[start + 1..end]));
                    }
                }
            }
        }
    }

    // Fallback: try common paths
    let fallback_paths = [
        "/usr/lib/zig/std",
        "/usr/local/lib/zig/std",
    ];

    for path in &fallback_paths {
        if Path::new(path).exists() {
            return Some(PathBuf::from(path));
        }
    }

    None
}
