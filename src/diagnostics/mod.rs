mod pretty;

pub use pretty::print_diagnostics;

use std::path::PathBuf;
use tower_lsp_server::ls_types::*;

#[derive(Debug, Clone)]
pub struct ZigDiagnostic {
    pub file: PathBuf,
    pub line: u32,
    pub column: u32,
    pub end_line: Option<u32>,
    pub end_column: Option<u32>,
    pub level: DiagnosticLevel,
    pub message: String,
    pub notes: Vec<DiagnosticNote>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DiagnosticLevel {
    Error,
    Warning,
    Note,
    Help,
}

impl std::fmt::Display for DiagnosticLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DiagnosticLevel::Error => write!(f, "error"),
            DiagnosticLevel::Warning => write!(f, "warning"),
            DiagnosticLevel::Note => write!(f, "note"),
            DiagnosticLevel::Help => write!(f, "help"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct DiagnosticNote {
    pub message: String,
    pub file: Option<PathBuf>,
    pub line: Option<u32>,
    pub column: Option<u32>,
}

pub fn zig_to_lsp_diagnostic(diag: ZigDiagnostic) -> Diagnostic {
    let start = Position::new(
        diag.line.saturating_sub(1),
        diag.column.saturating_sub(1),
    );

    let end = Position::new(
        diag.end_line.unwrap_or(diag.line).saturating_sub(1),
        diag.end_column.unwrap_or(diag.column + 1).saturating_sub(1),
    );

    let severity = match diag.level {
        DiagnosticLevel::Error => DiagnosticSeverity::ERROR,
        DiagnosticLevel::Warning => DiagnosticSeverity::WARNING,
        DiagnosticLevel::Note => DiagnosticSeverity::INFORMATION,
        DiagnosticLevel::Help => DiagnosticSeverity::HINT,
    };

    let related_information: Vec<DiagnosticRelatedInformation> = diag.notes.iter().filter_map(|note| {
        let note_path = note.file.as_ref().unwrap_or(&diag.file);
        let file_uri = Uri::from_file_path(note_path)?;
        let line = note.line.unwrap_or(diag.line).saturating_sub(1);
        let column = note.column.unwrap_or(diag.column).saturating_sub(1);

        Some(DiagnosticRelatedInformation {
            location: Location {
                uri: file_uri,
                range: Range {
                    start: Position::new(line, column),
                    end: Position::new(line, column + 1),
                },
            },
            message: note.message.clone(),
        })
    }).collect();

    Diagnostic {
        range: Range { start, end },
        severity: Some(severity),
        code: None,
        code_description: None,
        source: Some("zig".to_string()),
        message: diag.message,
        related_information: if related_information.is_empty() {
            None
        } else {
            Some(related_information)
        },
        tags: None,
        data: None,
    }
}
