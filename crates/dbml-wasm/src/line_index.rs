//! Mapeamento de coordenadas textuais bidirecionais entre offsets de bytes UTF-8 (Rowan)
//! e posições `(line, character)` em UTF-16 utilizadas pelo VS Code e browsers.

use serde::{Deserialize, Serialize};

/// Posição 0-based compatível com a especificação LSP / VS Code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Position {
    /// Linha baseada em 0
    pub line: u32,
    /// Coluna em unidades de código UTF-16 baseada em 0
    pub character: u32,
}

/// Textual range defined by UTF-16 start and end positions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Range {
    /// Inclusive start position.
    pub start: Position,
    /// Exclusive end position.
    pub end: Position,
}

/// Predominant line-ending style detected in a document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineEnding {
    /// Unix line feed (`\n`).
    Lf,
    /// Windows carriage-return + line feed (`\r\n`).
    Crlf,
}

impl LineEnding {
    /// Returns the line-ending string slice (`"\n"` or `"\r\n"`).
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            LineEnding::Lf => "\n",
            LineEnding::Crlf => "\r\n",
        }
    }
}

/// Índice de linhas com correspondência de UTF-8 e UTF-16.
#[derive(Debug, Clone)]
pub struct LineIndex {
    /// Offsets em bytes onde cada linha inicia
    line_starts: Vec<usize>,
    /// Comprimento total do texto em bytes
    len: usize,
    /// Quebra de linha detectada
    line_ending: LineEnding,
}

impl LineIndex {
    /// Builds a [`LineIndex`] for `text` by recording line-start byte offsets.
    #[must_use]
    pub fn new(text: &str) -> Self {
        let mut line_starts = vec![0];
        let bytes = text.as_bytes();
        let mut crlf_count = 0;
        let mut lf_count = 0;

        for (i, &b) in bytes.iter().enumerate() {
            if b == b'\n' {
                if i > 0 && bytes[i - 1] == b'\r' {
                    crlf_count += 1;
                } else {
                    lf_count += 1;
                }
                line_starts.push(i + 1);
            }
        }

        let line_ending = if crlf_count > lf_count {
            LineEnding::Crlf
        } else {
            LineEnding::Lf
        };

        Self {
            line_starts,
            len: text.len(),
            line_ending,
        }
    }

    /// Returns the predominant [`LineEnding`] detected in the indexed text.
    #[must_use]
    pub fn line_ending(&self) -> LineEnding {
        self.line_ending
    }

    /// Returns the total length of the indexed text in bytes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.len
    }

    /// Returns `true` if the indexed text is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Converte um offset de byte UTF-8 para uma posição `(line, character)` UTF-16.
    #[must_use]
    pub fn offset_to_position(&self, text: &str, offset: usize) -> Position {
        let mut offset = offset.min(self.len).min(text.len());
        while !text.is_char_boundary(offset) {
            offset -= 1;
        }

        // Busca binária para encontrar a linha do offset
        let line = match self.line_starts.binary_search(&offset) {
            Ok(idx) => idx,
            Err(idx) => idx.saturating_sub(1),
        };

        let mut line_start_offset = self.line_starts[line].min(offset);
        while !text.is_char_boundary(line_start_offset) {
            line_start_offset -= 1;
        }
        let line_slice = &text[line_start_offset..offset];

        // Conta código UTF-16 no slice da linha até o offset
        let mut utf16_col = 0u32;
        for c in line_slice.chars() {
            utf16_col += c.len_utf16() as u32;
        }

        Position {
            line: line as u32,
            character: utf16_col,
        }
    }

    /// Converte uma posição `(line, character)` UTF-16 para offset em bytes UTF-8.
    #[must_use]
    pub fn position_to_offset(&self, text: &str, pos: Position) -> Option<usize> {
        let line = pos.line as usize;
        if line >= self.line_starts.len() {
            return Some(self.len);
        }

        let line_start = self.line_starts[line];
        let next_line_start = self.line_starts.get(line + 1).copied().unwrap_or(self.len);

        let line_text = &text[line_start..next_line_start];
        let mut cur_utf16 = 0u32;
        let mut byte_offset_in_line = 0usize;

        for c in line_text.chars() {
            if cur_utf16 >= pos.character {
                break;
            }
            cur_utf16 += c.len_utf16() as u32;
            byte_offset_in_line += c.len_utf8();
        }

        Some((line_start + byte_offset_in_line).min(self.len))
    }

    /// Converte um range de bytes (`std::ops::Range<usize>`) para Range LSP (UTF-16).
    #[must_use]
    pub fn byte_range_to_range(&self, text: &str, r: std::ops::Range<usize>) -> Range {
        Range {
            start: self.offset_to_position(text, r.start),
            end: self.offset_to_position(text, r.end),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ascii_coordinates() {
        let text = "Table users {\n  id int\n  name varchar\n}";
        let idx = LineIndex::new(text);

        assert_eq!(idx.line_ending(), LineEnding::Lf);
        assert_eq!(
            idx.offset_to_position(text, 0),
            Position {
                line: 0,
                character: 0
            }
        );
        // Na segunda linha: "  id int"
        let pos = idx.offset_to_position(text, 14); // logo após a quebra de linha
        assert_eq!(
            pos,
            Position {
                line: 1,
                character: 0
            }
        );
    }

    #[test]
    fn test_multibyte_utf8_and_emojis() {
        // "usuário" tem acento ('á' tem 2 bytes em UTF-8, mas 1 code unit em UTF-16)
        // 🚀 tem 4 bytes em UTF-8, mas 2 code units em UTF-16
        let text = "Table usuário 🚀 {\n  descrição text\n}";
        let idx = LineIndex::new(text);

        // Início da segunda linha
        let second_line_start = text.find("descrição").unwrap();
        let pos = idx.offset_to_position(text, second_line_start);
        assert_eq!(pos.line, 1);

        // Conversão reversa exata
        let back_offset = idx.position_to_offset(text, pos).unwrap();
        assert_eq!(back_offset, second_line_start);
    }

    #[test]
    fn test_crlf_detection() {
        let text = "Table a {\r\n  id int\r\n}\r\n";
        let idx = LineIndex::new(text);
        assert_eq!(idx.line_ending(), LineEnding::Crlf);
    }
}
