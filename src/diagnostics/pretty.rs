use std::path::Path;
use std::fs;

use ariadne::{Color, ColorGenerator, Fmt, Label, Report, ReportKind, Source};

use super::{ZigDiagnostic, DiagnosticLevel};

pub fn print_diagnostics(diagnostics: &[ZigDiagnostic], default_file: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if diagnostics.is_empty() {
        return Ok(());
    }

    let mut by_file: std::collections::HashMap<&Path, Vec<&ZigDiagnostic>> = std::collections::HashMap::new();
    for diag in diagnostics {
        by_file.entry(&diag.file).or_default().push(diag);
    }

    let mut colors = ColorGenerator::new();

    for (file, file_diags) in &by_file {
        let file_path = if file.exists() {
            file.to_path_buf()
        } else {
            default_file.to_path_buf()
        };

        let source_text = fs::read_to_string(&file_path)
            .unwrap_or_default();

        let file_name = file_path.to_string_lossy().into_owned();
        let source = Source::from(source_text.as_str());

        for diag in file_diags {
            let kind = match diag.level {
                DiagnosticLevel::Error => ReportKind::Error,
                DiagnosticLevel::Warning => ReportKind::Warning,
                DiagnosticLevel::Note => ReportKind::Advice,
                DiagnosticLevel::Help => ReportKind::Advice,
            };

            let color = match diag.level {
                DiagnosticLevel::Error => Color::Red,
                DiagnosticLevel::Warning => Color::Yellow,
                DiagnosticLevel::Note => Color::Cyan,
                DiagnosticLevel::Help => Color::Green,
            };

            let start_offset = offset_from_line_col(&source_text, diag.line, diag.column);
            let end_offset = diag.end_line
                .map(|l| offset_from_line_col(&source_text, l, diag.end_column.unwrap_or(diag.column + 1)))
                .unwrap_or(start_offset);

            let mut report = Report::build(kind, (&*file_name, start_offset..end_offset))
                .with_message(&diag.message);

            report = report.with_label(
                Label::new((&*file_name, start_offset..end_offset))
                    .with_message(format!("{}", diag.level.clone().fg(color)))
                    .with_color(color),
            );

            let mut note_labels: Vec<(String, usize, String, ariadne::Color)> = Vec::new();

            for note in &diag.notes {
                if let (Some(note_file), Some(note_line), Some(note_col)) = (&note.file, note.line, note.column) {
                    let note_offset = if note_file.exists() {
                        let note_source = fs::read_to_string(note_file).unwrap_or_default();
                        offset_from_line_col(&note_source, note_line, note_col)
                    } else {
                        offset_from_line_col(&source_text, note_line, note_col)
                    };

                    let note_name = note_file.to_string_lossy().into_owned();
                    note_labels.push((note_name, note_offset, note.message.clone(), colors.next()));
                } else {
                    report = report.with_note(&note.message);
                }
            }

            for (note_name, note_offset, note_msg, note_color) in &note_labels {
                report = report.with_label(
                    Label::new((note_name.as_str(), *note_offset..*note_offset + 1))
                        .with_message(note_msg.as_str())
                        .with_color(*note_color),
                );
            }

            let mut out = Vec::new();
            report.finish()
                .write((&*file_name, &source), &mut out)
                .ok();
            eprint!("{}", String::from_utf8_lossy(&out));
        }
    }

    Ok(())
}

fn offset_from_line_col(source: &str, line: u32, column: u32) -> usize {
    let target_line = line.saturating_sub(1) as usize;
    let target_col = column.saturating_sub(1) as usize;

    let mut current_line = 0;

    for (i, ch) in source.char_indices() {
        if current_line == target_line {
            let line_start = i;
            let mut byte_count = 0;
            let mut char_count = 0;

            for (_, c) in source[line_start..].char_indices() {
                if char_count >= target_col {
                    break;
                }
                byte_count += c.len_utf8();
                char_count += 1;
            }

            return line_start + byte_count;
        }

        if ch == '\n' {
            current_line += 1;
        }
    }

    source.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_offset_from_line_col() {
        let source = "line 1\nline 2\nline 3";
        assert_eq!(offset_from_line_col(source, 1, 1), 0);
        assert_eq!(offset_from_line_col(source, 1, 6), 5);
        assert_eq!(offset_from_line_col(source, 2, 1), 7);
        assert_eq!(offset_from_line_col(source, 2, 6), 12);
    }
}
