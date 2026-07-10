use tower_lsp_server::ls_types::*;
use ropey::Rope;

#[derive(Debug, Clone)]
pub struct Document {
    uri: Uri,
    rope: Rope,
    version: i32,
}

#[allow(dead_code)]
impl Document {
    pub fn new(uri: Uri, text: String, version: i32) -> Self {
        let rope = Rope::from_str(&text);
        Self {
            uri,
            rope,
            version,
        }
    }

    pub fn update(&mut self, text: &str, version: i32) {
        self.rope = Rope::from_str(text);
        self.version = version;
    }

    pub fn get_text(&self) -> String {
        self.rope.to_string()
    }

    pub fn get_rope(&self) -> &Rope {
        &self.rope
    }

    pub fn version(&self) -> i32 {
        self.version
    }

    pub fn uri(&self) -> &Uri {
        &self.uri
    }

    pub fn line_count(&self) -> usize {
        self.rope.len_lines()
    }

    pub fn char_count(&self) -> usize {
        self.rope.len_chars()
    }

    pub fn position_to_offset(&self, position: Position) -> usize {
        let line = position.line as usize;
        let column = position.character as usize;

        let line_start = self.rope.line_to_char(line);
        let line_text = self.rope.line(line).to_string();

        let mut char_offset = 0;
        let mut byte_offset = 0;

        for ch in line_text.chars() {
            if byte_offset >= column {
                break;
            }
            byte_offset += ch.len_utf8();
            char_offset += 1;
        }

        line_start + char_offset
    }

    pub fn offset_to_position(&self, offset: usize) -> Position {
        let line = self.rope.char_to_line(offset);
        let line_start = self.rope.line_to_char(line);
        let column = offset - line_start;

        Position::new(line as u32, column as u32)
    }

    pub fn get_line(&self, line: usize) -> String {
        if line >= self.rope.len_lines() {
            return String::new();
        }
        self.rope.line(line).to_string()
    }

    pub fn get_range(&self, range: Range) -> String {
        let start = self.position_to_offset(range.start);
        let end = self.position_to_offset(range.end);

        if start >= end || start >= self.rope.len_chars() {
            return String::new();
        }

        let end = end.min(self.rope.len_chars());
        self.rope.slice(start..end).to_string()
    }

    #[allow(deprecated)]
    pub fn extract_symbols(&self) -> Vec<DocumentSymbol> {
        let mut symbols = Vec::new();
        let text = self.get_text();
        let lines: Vec<&str> = text.lines().collect();

        for (line_idx, line) in lines.iter().enumerate() {
            let trimmed = line.trim();

            if trimmed.starts_with("//") || trimmed.is_empty() {
                continue;
            }

            if let Some(name) = extract_symbol_name(trimmed, "fn") {
                symbols.push(DocumentSymbol {
                    name,
                    detail: Some("function".to_string()),
                    kind: SymbolKind::FUNCTION,
                    tags: None,
                    deprecated: None,
                    range: Range {
                        start: Position::new(line_idx as u32, 0),
                        end: Position::new(line_idx as u32, line.len() as u32),
                    },
                    selection_range: Range {
                        start: Position::new(line_idx as u32, 0),
                        end: Position::new(line_idx as u32, line.len() as u32),
                    },
                    children: None,
                });
            }

            if let Some(name) = extract_symbol_name(trimmed, "struct") {
                symbols.push(DocumentSymbol {
                    name,
                    detail: Some("struct".to_string()),
                    kind: SymbolKind::STRUCT,
                    tags: None,
                    deprecated: None,
                    range: Range {
                        start: Position::new(line_idx as u32, 0),
                        end: Position::new(line_idx as u32, line.len() as u32),
                    },
                    selection_range: Range {
                        start: Position::new(line_idx as u32, 0),
                        end: Position::new(line_idx as u32, line.len() as u32),
                    },
                    children: None,
                });
            }

            if let Some(name) = extract_symbol_name(trimmed, "enum") {
                symbols.push(DocumentSymbol {
                    name,
                    detail: Some("enum".to_string()),
                    kind: SymbolKind::ENUM,
                    tags: None,
                    deprecated: None,
                    range: Range {
                        start: Position::new(line_idx as u32, 0),
                        end: Position::new(line_idx as u32, line.len() as u32),
                    },
                    selection_range: Range {
                        start: Position::new(line_idx as u32, 0),
                        end: Position::new(line_idx as u32, line.len() as u32),
                    },
                    children: None,
                });
            }

            if let Some(name) = extract_const_name(trimmed) {
                symbols.push(DocumentSymbol {
                    name,
                    detail: Some("constant".to_string()),
                    kind: SymbolKind::CONSTANT,
                    tags: None,
                    deprecated: None,
                    range: Range {
                        start: Position::new(line_idx as u32, 0),
                        end: Position::new(line_idx as u32, line.len() as u32),
                    },
                    selection_range: Range {
                        start: Position::new(line_idx as u32, 0),
                        end: Position::new(line_idx as u32, line.len() as u32),
                    },
                    children: None,
                });
            }

            if let Some(name) = extract_var_name(trimmed) {
                symbols.push(DocumentSymbol {
                    name,
                    detail: Some("variable".to_string()),
                    kind: SymbolKind::VARIABLE,
                    tags: None,
                    deprecated: None,
                    range: Range {
                        start: Position::new(line_idx as u32, 0),
                        end: Position::new(line_idx as u32, line.len() as u32),
                    },
                    selection_range: Range {
                        start: Position::new(line_idx as u32, 0),
                        end: Position::new(line_idx as u32, line.len() as u32),
                    },
                    children: None,
                });
            }
        }

        symbols
    }
}

fn extract_symbol_name(line: &str, keyword: &str) -> Option<String> {
    if let Some(start) = line.find(&format!("{} ", keyword)) {
        let rest = &line[start + keyword.len()..].trim_start();
        if let Some(end) = rest.find(|c: char| !c.is_alphanumeric() && c != '_') {
            let name = &rest[..end];
            if !name.is_empty() {
                return Some(name.to_string());
            }
        } else if !rest.is_empty() {
            return Some(rest.to_string());
        }
    }
    None
}

fn extract_const_name(line: &str) -> Option<String> {
    if line.starts_with("const ") || line.starts_with("pub const ") {
        let rest = if line.starts_with("pub const ") {
            &line["pub const ".len()..]
        } else {
            &line["const ".len()..]
        };

        if let Some(end) = rest.find(|c: char| !c.is_alphanumeric() && c != '_') {
            let name = &rest[..end];
            if !name.is_empty() {
                return Some(name.to_string());
            }
        }
    }
    None
}

fn extract_var_name(line: &str) -> Option<String> {
    if line.starts_with("var ") || line.starts_with("pub var ") {
        let rest = if line.starts_with("pub var ") {
            &line["pub var ".len()..]
        } else {
            &line["var ".len()..]
        };

        if let Some(end) = rest.find(|c: char| !c.is_alphanumeric() && c != '_') {
            let name = &rest[..end];
            if !name.is_empty() {
                return Some(name.to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_document_new() {
        let uri = Uri::from_file_path("/test.zig").unwrap();
        let text = "const x = 42;".to_string();
        let doc = Document::new(uri, text, 1);

        assert_eq!(doc.get_text(), "const x = 42;");
        assert_eq!(doc.version(), 1);
        assert_eq!(doc.line_count(), 1);
    }

    #[test]
    fn test_position_conversion() {
        let uri = Uri::from_file_path("/test.zig").unwrap();
        let text = "line 1\nline 2\nline 3".to_string();
        let doc = Document::new(uri, text, 1);

        let pos = Position::new(1, 2);
        let offset = doc.position_to_offset(pos);
        let back = doc.offset_to_position(offset);

        assert_eq!(pos, back);
    }

    #[test]
    fn test_extract_symbols() {
        let uri = Uri::from_file_path("/test.zig").unwrap();
        let text = r#"
const MAX_SIZE: u32 = 1024;

pub fn main() void {
    var x: u32 = 0;
    const result = add(x, 1);
}

fn add(a: u32, b: u32) u32 {
    return a + b;
}

struct Point {
    x: f32,
    y: f32,
}

const Direction = enum {
    North,
    South,
    East,
    West,
};
"#.to_string();

        let doc = Document::new(uri, text, 1);
        let symbols = doc.extract_symbols();

        assert!(!symbols.is_empty());

        let symbol_names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(symbol_names.contains(&"main"));
        assert!(symbol_names.contains(&"add"));
        assert!(symbol_names.contains(&"Point"));
        assert!(symbol_names.contains(&"Direction"));
        assert!(symbol_names.contains(&"MAX_SIZE"));
    }
}
