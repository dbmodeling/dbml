#![forbid(unsafe_code)]
#![deny(missing_docs)]
//! Lossless lexer (`logos`) and error-recovering CST parser (`rowan`) for DBML.
//!
//! Invariant: `parse(text).syntax().text() == text` for any input.

mod lexer;
mod parser;

pub use lexer::lex;
pub use parser::{Parse, parse};

use rowan::Language;

/// Token and syntax-node kinds in the DBML concrete syntax tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u16)]
pub enum SyntaxKind {
    // Tokens
    /// Horizontal whitespace (` `, `\t`, `\r`).
    Whitespace,
    /// Line feed (`\n`).
    Newline,
    /// Single-line comment (`// ...`).
    Comment,
    /// Identifier (`[A-Za-z_][A-Za-z0-9_]*`).
    Ident,
    /// Quoted string literal (`"..."`, `'...'`, or `` `...` ``).
    Str,
    /// Numeric literal.
    Number,
    /// Opening brace `{`.
    LBrace,
    /// Closing brace `}`.
    RBrace,
    /// Opening bracket `[`.
    LBracket,
    /// Closing bracket `]`.
    RBracket,
    /// Opening parenthesis `(`.
    LParen,
    /// Closing parenthesis `)`.
    RParen,
    /// Comma `,`.
    Comma,
    /// Colon `:`.
    Colon,
    /// Any unrecognized character sequence preserved for lossless recovery.
    Other,
    // Composite CST nodes
    /// Root CST node wrapping the entire document.
    Root,
    /// `Table name { ... }` node.
    Table,
    /// `TableGroup name { ... }` node.
    TableGroup,
    /// Top-level `Ref ...` node.
    Ref,
    /// Column declaration line inside a `Table`.
    Column,
    /// Nested `{ ... }` block such as `indexes { ... }` or `Note { ... }`.
    Block,
    /// Unparsed/unexpected token sequence preserved losslessly.
    Error,
}

impl SyntaxKind {
    /// Returns `true` for trivia tokens (`Whitespace`, `Newline`, `Comment`).
    #[must_use]
    pub fn is_trivia(self) -> bool {
        matches!(self, Self::Whitespace | Self::Newline | Self::Comment)
    }
}

/// `rowan::Language` marker type for DBML CSTs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DbmlLang {}

impl Language for DbmlLang {
    type Kind = SyntaxKind;
    fn kind_from_raw(raw: rowan::SyntaxKind) -> SyntaxKind {
        match raw.0 {
            0 => SyntaxKind::Whitespace,
            1 => SyntaxKind::Newline,
            2 => SyntaxKind::Comment,
            3 => SyntaxKind::Ident,
            4 => SyntaxKind::Str,
            5 => SyntaxKind::Number,
            6 => SyntaxKind::LBrace,
            7 => SyntaxKind::RBrace,
            8 => SyntaxKind::LBracket,
            9 => SyntaxKind::RBracket,
            10 => SyntaxKind::LParen,
            11 => SyntaxKind::RParen,
            12 => SyntaxKind::Comma,
            13 => SyntaxKind::Colon,
            14 => SyntaxKind::Other,
            15 => SyntaxKind::Root,
            16 => SyntaxKind::Table,
            17 => SyntaxKind::TableGroup,
            18 => SyntaxKind::Ref,
            19 => SyntaxKind::Column,
            20 => SyntaxKind::Block,
            21 => SyntaxKind::Error,
            other => panic!("invalid SyntaxKind raw discriminant: {other}"),
        }
    }
    fn kind_to_raw(kind: SyntaxKind) -> rowan::SyntaxKind {
        rowan::SyntaxKind(kind as u16)
    }
}

/// Typed `rowan::SyntaxNode` for [`DbmlLang`].
pub type SyntaxNode = rowan::SyntaxNode<DbmlLang>;
/// Typed `rowan::SyntaxToken` for [`DbmlLang`].
pub type SyntaxToken = rowan::SyntaxToken<DbmlLang>;

/// Strips surrounding DBML quotes (`"`, `'`, or `` ` ``) from an identifier or literal.
#[must_use]
pub fn unquote(s: &str) -> &str {
    s.trim_matches(|c| c == '"' || c == '\'' || c == '`')
}

/// Collects direct non-trivia child tokens of `node`.
#[must_use]
pub fn child_tokens(node: &SyntaxNode) -> Vec<SyntaxToken> {
    node.children_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .filter(|t| !t.kind().is_trivia())
        .collect()
}

/// Collects all non-trivia descendant tokens of `node` in source order.
#[must_use]
pub fn descendant_tokens(node: &SyntaxNode) -> Vec<SyntaxToken> {
    node.descendants_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .filter(|t| !t.kind().is_trivia())
        .collect()
}
