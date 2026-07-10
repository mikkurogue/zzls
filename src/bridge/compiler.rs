use std::path::{Path, PathBuf};
use std::process::Stdio;

use tokio::process::Command;

use crate::diagnostics::ZigDiagnostic;

#[derive(Debug, Clone)]
pub struct ZigCompiler {
    zig_path: PathBuf,
}

impl ZigCompiler {
    pub fn new(zig_path: PathBuf) -> Self {
        Self { zig_path }
    }

    /// Check a file using `zig build-exe -fno-emit-bin`
    pub async fn check(&self, file: &Path) -> Result<Vec<ZigDiagnostic>, CompilerError> {
        let source = std::fs::read_to_string(file)
            .map_err(|e| CompilerError::Io(e.to_string()))?;

        self.check_source(&source, file).await
    }

    /// Check source text using `zig ast-check` for fast syntax validation,
    /// then `zig build-exe` for full semantic checking. Returns combined diagnostics.
    pub async fn check_source(&self, _source: &str, file: &Path) -> Result<Vec<ZigDiagnostic>, CompilerError> {
        let (ast_result, build_result) = tokio::join!(
            self.ast_check(file),
            self.build_check(file)
        );

        let mut diagnostics = Vec::new();
        let mut seen = std::collections::HashSet::new();

        for diag in ast_result.unwrap_or_default() {
            let key = (diag.line, diag.column, diag.message.clone());
            if seen.insert(key) {
                diagnostics.push(diag);
            }
        }

        for diag in build_result.unwrap_or_default() {
            let key = (diag.line, diag.column, diag.message.clone());
            if seen.insert(key) {
                diagnostics.push(diag);
            }
        }

        Ok(diagnostics)
    }

    /// Run `zig ast-check <file>` for fast syntax validation
    async fn ast_check(&self, file: &Path) -> Result<Vec<ZigDiagnostic>, CompilerError> {
        let output = Command::new(&self.zig_path)
            .arg("ast-check")
            .arg(file)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| CompilerError::Process(e.to_string()))?;

        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);

        if output.status.success() {
            return Ok(vec![]);
        }

        // Parse the error output
        let combined = format!("{}\n{}", stderr, stdout);
        let diagnostics = parse_zig_diagnostics(&combined, file);

        Ok(diagnostics)
    }

    /// Run `zig build-exe -fno-emit-bin <file>` for full semantic checking
    async fn build_check(&self, file: &Path) -> Result<Vec<ZigDiagnostic>, CompilerError> {
        let output = Command::new(&self.zig_path)
            .arg("build-exe")
            .arg(file)
            .arg("-fno-emit-bin")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| CompilerError::Process(e.to_string()))?;

        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);

        if output.status.success() {
            return Ok(vec![]);
        }

        // Parse the error output
        let combined = format!("{}\n{}", stderr, stdout);
        let diagnostics = parse_zig_diagnostics(&combined, file);

        Ok(diagnostics)
    }
}

/// Parse zig error output (multi-line aware)
fn parse_zig_diagnostics(output: &str, default_file: &Path) -> Vec<ZigDiagnostic> {
    let mut diagnostics = Vec::new();
    let lines: Vec<&str> = output.lines().collect();
    let mut i = 0;

    // Messages that benefit from having the identifier name appended
    let append_name_msgs = ["unused", "redundant", "shadow", "undeclared", "not defined"];

    while i < lines.len() {
        if let Some(mut diag) = parse_zig_error_line(lines[i], default_file) {
            // Look ahead for code line and caret line to extract identifier name
            if i + 2 < lines.len() {
                let code_line = lines[i + 1];
                let caret_line = lines[i + 2];

                let should_append = append_name_msgs.iter().any(|m| diag.message.contains(m));
                if should_append {
                    if let Some(name) = extract_identifier_from_caret(code_line, caret_line, diag.column) {
                        diag.message = format!("{}: {}", diag.message, name);
                    }
                }
            }
            diagnostics.push(diag);
        }
        i += 1;
    }

    diagnostics
}

/// Extract identifier name from zig's caret markers
/// The caret line looks like: "          ^~~~~~~~~~~~"
/// Zig uses ^ for start and ~ for continuation of the span
fn extract_identifier_from_caret(code_line: &str, caret_line: &str, _col: u32) -> Option<String> {
    // Find the first caret position (byte index)
    let caret_start = caret_line.find('^')?;
    // Count ^ and ~ as continuation markers
    let caret_len = caret_line[caret_start..].chars().take_while(|&c| c == '^' || c == '~').count();

    // For ASCII zig code, byte index = char index
    let name: String = code_line.chars().skip(caret_start).take(caret_len).collect();
    let name = name.trim().to_string();

    // Only return if it looks like an identifier
    if name.is_empty() || name.chars().next().map_or(false, |c| c.is_ascii_digit()) {
        return None;
    }

    Some(name)
}

/// Parse a single line of zig error output
fn parse_zig_error_line(line: &str, default_file: &Path) -> Option<ZigDiagnostic> {
    // Zig errors look like:
    // file.zig:10:5: error: message here
    // file.zig:10:5: note: message here
    let parts: Vec<&str> = line.splitn(4, ':').collect();
    if parts.len() < 4 {
        return None;
    }

    let file_str = parts[0].trim();
    let line_num: u32 = parts[1].trim().parse().ok()?;
    let col_num: u32 = parts[2].trim().parse().ok()?;

    let rest = parts[3].trim();
    let (level, message) = if rest.starts_with("error:") {
        (crate::diagnostics::DiagnosticLevel::Error, rest[6..].trim().to_string())
    } else if rest.starts_with("warning:") {
        (crate::diagnostics::DiagnosticLevel::Warning, rest[8..].trim().to_string())
    } else if rest.starts_with("note:") {
        (crate::diagnostics::DiagnosticLevel::Note, rest[5..].trim().to_string())
    } else if rest.starts_with("help:") {
        (crate::diagnostics::DiagnosticLevel::Help, rest[5..].trim().to_string())
    } else {
        return None;
    };

    let path = if file_str == "" || file_str == "<input>" {
        default_file.to_path_buf()
    } else {
        PathBuf::from(file_str)
    };

    Some(ZigDiagnostic {
        file: path,
        line: line_num,
        column: col_num,
        end_line: None,
        end_column: None,
        level,
        message,
        notes: vec![],
    })
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum CompilerError {
    Io(String),
    Process(String),
    Parse(String),
}

impl std::fmt::Display for CompilerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CompilerError::Io(e) => write!(f, "IO error: {}", e),
            CompilerError::Process(e) => write!(f, "Process error: {}", e),
            CompilerError::Parse(e) => write!(f, "Parse error: {}", e),
        }
    }
}

impl std::error::Error for CompilerError {}
