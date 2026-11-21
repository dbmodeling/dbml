use crate::SyntaxKind;
use logos::Logos;

#[derive(Logos, Debug, Clone, Copy, PartialEq)]
enum Tok {
    // U+FEFF (BOM UTF-8) é trivia: arquivos salvos com BOM continuam com o mesmo CST.
    #[regex("[ \t\r\u{feff}]+")]
    Whitespace,
    #[token("\n")]
    Newline,
    #[regex(r"//[^\n]*", allow_greedy = true)]
    Comment,
    #[regex(r"[A-Za-z_][A-Za-z0-9_]*")]
    Ident,
    #[regex(r#""[^"\n]*"?"#)]
    #[regex(r"'[^'\n]*'?")]
    #[regex(r"`[^`]*`?")]
    // DBML multi-line string: may contain `'`, `''` and newlines, closes at the first `'''`.
    #[regex(r"'''([^']|'[^']|''[^'])*'''")]
    Str,
    #[regex(r"[0-9]+(\.[0-9]+)?")]
    Number,
    #[token("{")]
    LBrace,
    #[token("}")]
    RBrace,
    #[token("[")]
    LBracket,
    #[token("]")]
    RBracket,
    #[token("(")]
    LParen,
    #[token(")")]
    RParen,
    #[token(",")]
    Comma,
    #[token(":")]
    Colon,
}

/// Losslessly tokenizes `text`; unrecognized spans become [`SyntaxKind::Other`].
#[must_use]
pub fn lex(text: &str) -> Vec<(SyntaxKind, &str)> {
    let mut out = Vec::new();
    for (tok, span) in Tok::lexer(text).spanned() {
        let kind = match tok {
            Ok(Tok::Whitespace) => SyntaxKind::Whitespace,
            Ok(Tok::Newline) => SyntaxKind::Newline,
            Ok(Tok::Comment) => SyntaxKind::Comment,
            Ok(Tok::Ident) => SyntaxKind::Ident,
            Ok(Tok::Str) => SyntaxKind::Str,
            Ok(Tok::Number) => SyntaxKind::Number,
            Ok(Tok::LBrace) => SyntaxKind::LBrace,
            Ok(Tok::RBrace) => SyntaxKind::RBrace,
            Ok(Tok::LBracket) => SyntaxKind::LBracket,
            Ok(Tok::RBracket) => SyntaxKind::RBracket,
            Ok(Tok::LParen) => SyntaxKind::LParen,
            Ok(Tok::RParen) => SyntaxKind::RParen,
            Ok(Tok::Comma) => SyntaxKind::Comma,
            Ok(Tok::Colon) => SyntaxKind::Colon,
            Err(()) => SyntaxKind::Other,
        };
        out.push((kind, &text[span]));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn triple_quoted_string_is_one_token() {
        let toks = lex("Note: '''it's\nmulti'''\n");
        assert_eq!(toks[3], (SyntaxKind::Str, "'''it's\nmulti'''"));
    }

    #[test]
    fn leading_bom_is_whitespace_trivia() {
        let toks = lex("\u{feff}Table t {}");
        assert_eq!(toks[0], (SyntaxKind::Whitespace, "\u{feff}"));
        assert_eq!(toks[1], (SyntaxKind::Ident, "Table"));
        assert!(!toks.iter().any(|(k, _)| *k == SyntaxKind::Other));
    }

    #[test]
    fn empty_and_unterminated_quotes_still_lex() {
        assert_eq!(lex("''")[0], (SyntaxKind::Str, "''"));
        assert_eq!(lex("'''abc")[0], (SyntaxKind::Str, "''"));
    }
}
