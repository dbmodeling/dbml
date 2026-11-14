//! Zero-copy, never-failing SQL lexer.

use crate::SqlDialect;
use std::borrow::Cow;

/// Lexical category of a [`Token`]; text is borrowed from the source whenever possible.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TokenKind<'a> {
    /// Unquoted identifier or keyword (original case preserved).
    Ident(&'a str),
    /// Quoted identifier (`"x"`, `` `x` ``, `[x]`) with delimiters removed and doubled quotes unescaped.
    Quoted(Cow<'a, str>),
    /// Single-quoted string literal (contents, doubled quotes unescaped).
    Str(Cow<'a, str>),
    /// Postgres dollar-quoted body (`$$..$$`, `$tag$..$tag$`), delimiters removed.
    DollarStr(&'a str),
    /// Numeric literal (also swallows exponent / suffix letters).
    Number(&'a str),
    /// Single punctuation character: `( ) , ; . = * + - < > / ] [ :`.
    Punct(char),
    /// Anything the lexer does not understand.
    Unknown(char),
}

/// Token with the 1-based position of its first character.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Token<'a> {
    pub(crate) kind: TokenKind<'a>,
    pub(crate) line: u32,
    pub(crate) col: u32,
}

impl Token<'_> {
    /// Case-insensitive keyword test; only unquoted identifiers can be keywords.
    pub(crate) fn is_kw(&self, kw: &str) -> bool {
        matches!(self.kind, TokenKind::Ident(s) if s.eq_ignore_ascii_case(kw))
    }
}

/// Lexes `src`; unterminated literals and comments run to end of input. Never fails.
pub(crate) fn lex(src: &str, dialect: SqlDialect) -> Vec<Token<'_>> {
    Lexer {
        src,
        dialect,
        pos: 0,
        line: 1,
        col: 1,
        in_mysql_conditional: false,
    }
    .run()
}

struct Lexer<'a> {
    src: &'a str,
    dialect: SqlDialect,
    pos: usize,
    line: u32,
    col: u32,
    in_mysql_conditional: bool,
}

impl<'a> Lexer<'a> {
    fn rest(&self) -> &'a str {
        self.src.get(self.pos..).unwrap_or("")
    }

    fn peek(&self) -> Option<char> {
        self.rest().chars().next()
    }

    fn peek2(&self) -> Option<char> {
        self.rest().chars().nth(1)
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += c.len_utf8();
        if c == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(c)
    }

    fn bump_while(&mut self, pred: impl Fn(char) -> bool) {
        while self.peek().is_some_and(&pred) {
            self.bump();
        }
    }

    fn bump_to(&mut self, end: usize) {
        while self.pos < end && self.bump().is_some() {}
    }

    fn slice_from(&self, start: usize) -> &'a str {
        self.src.get(start..self.pos).unwrap_or("")
    }

    fn run(mut self) -> Vec<Token<'a>> {
        let mut out = Vec::new();
        while let Some(c) = self.peek() {
            let (line, col) = (self.line, self.col);
            if let Some(kind) = self.next_kind(c) {
                out.push(Token { kind, line, col });
            }
        }
        out
    }

    /// Consumes one lexical unit starting at `c`; returns `None` for trivia (whitespace/comments).
    fn next_kind(&mut self, c: char) -> Option<TokenKind<'a>> {
        let mysql = self.dialect == SqlDialect::Mysql;
        let postgres = self.dialect == SqlDialect::Postgres;
        match c {
            _ if c.is_whitespace() => {
                self.bump();
                None
            }
            '-' if self.peek2() == Some('-') => {
                self.bump_while(|c| c != '\n');
                None
            }
            '#' if mysql => {
                self.bump_while(|c| c != '\n');
                None
            }
            '/' if self.peek2() == Some('*') => {
                self.block_comment();
                None
            }
            '*' if self.in_mysql_conditional && self.peek2() == Some('/') => {
                self.in_mysql_conditional = false;
                self.bump();
                self.bump();
                None
            }
            '\'' => Some(TokenKind::Str(self.delimited('\'', mysql))),
            'E' | 'e' if postgres && self.peek2() == Some('\'') => {
                self.bump();
                Some(TokenKind::Str(self.delimited('\'', true)))
            }
            '"' if mysql => Some(TokenKind::Str(self.delimited('"', true))),
            '"' => Some(TokenKind::Quoted(self.delimited('"', false))),
            '`' if mysql => Some(TokenKind::Quoted(self.delimited('`', false))),
            '[' if self.dialect == SqlDialect::Sqlite => {
                Some(TokenKind::Quoted(self.delimited(']', false)))
            }
            '$' if postgres => Some(self.dollar()),
            _ if c.is_ascii_digit() => {
                let start = self.pos;
                self.bump_while(|c| c.is_alphanumeric() || c == '.' || c == '_');
                Some(TokenKind::Number(self.slice_from(start)))
            }
            _ if c.is_alphabetic() || c == '_' => {
                let start = self.pos;
                self.bump_while(|c| c.is_alphanumeric() || c == '_' || c == '$');
                Some(TokenKind::Ident(self.slice_from(start)))
            }
            '(' | ')' | ',' | ';' | '.' | '=' | '*' | '+' | '-' | '<' | '>' | '/' | '[' | ']'
            | ':' => {
                self.bump();
                Some(TokenKind::Punct(c))
            }
            _ => {
                self.bump();
                Some(TokenKind::Unknown(c))
            }
        }
    }

    fn block_comment(&mut self) {
        self.bump();
        self.bump();
        // MySQL `/*! .. */` and `/*!40101 .. */` hold executable code: lex their content.
        if self.dialect == SqlDialect::Mysql && self.peek() == Some('!') {
            self.bump();
            self.bump_while(|c| c.is_ascii_digit());
            self.in_mysql_conditional = true;
            return;
        }
        while let Some(c) = self.bump() {
            if c == '*' && self.peek() == Some('/') {
                self.bump();
                return;
            }
        }
    }

    /// Reads a literal whose opening delimiter is the current character and consumes it.
    /// Doubled `close` is an escaped delimiter. With `backslash`, `\x` never terminates the
    /// literal (backslash escapes are left raw in the contents).
    fn delimited(&mut self, close: char, backslash: bool) -> Cow<'a, str> {
        // Opening delimiter: consume it.
        self.bump();
        let start = self.pos;
        let mut end = None;
        while let Some(c) = self.peek() {
            if backslash && c == '\\' {
                self.bump();
                self.bump();
            } else if c == close {
                if self.peek2() == Some(close) {
                    self.bump();
                    self.bump();
                } else {
                    end = Some(self.pos);
                    self.bump();
                    break;
                }
            } else {
                self.bump();
            }
        }
        let raw = self.src.get(start..end.unwrap_or(self.pos)).unwrap_or("");
        let doubled: String = [close, close].iter().collect();
        if raw.contains(doubled.as_str()) {
            Cow::Owned(raw.replace(doubled.as_str(), &close.to_string()))
        } else {
            Cow::Borrowed(raw)
        }
    }

    /// `$$..$$` / `$tag$..$tag$`; a lone `$` (e.g. `$1`) becomes `Unknown('$')`.
    fn dollar(&mut self) -> TokenKind<'a> {
        let after = self.rest().get(1..).unwrap_or("");
        let tag_len = after
            .find(|c: char| !(c.is_alphanumeric() || c == '_'))
            .unwrap_or(after.len());
        let tag = after.get(..tag_len).unwrap_or("");
        let valid = after.get(tag_len..).is_some_and(|r| r.starts_with('$'))
            && !tag.starts_with(|c: char| c.is_ascii_digit());
        if !valid {
            self.bump();
            return TokenKind::Unknown('$');
        }
        let delim_len = tag_len + 2;
        let body_start = self.pos + delim_len;
        let body = self.src.get(body_start..).unwrap_or("");
        let delim = self.src.get(self.pos..body_start).unwrap_or("$$");
        if let Some(rel) = body.find(delim) {
            let body_end = body_start + rel;
            let text = self.src.get(body_start..body_end).unwrap_or("");
            self.bump_to(body_end + delim_len);
            TokenKind::DollarStr(text)
        } else {
            self.bump_to(self.src.len());
            TokenKind::DollarStr(body)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const ALL: [SqlDialect; 3] = [SqlDialect::Postgres, SqlDialect::Mysql, SqlDialect::Sqlite];

    fn kinds(src: &str, d: SqlDialect) -> Vec<TokenKind<'_>> {
        lex(src, d).into_iter().map(|t| t.kind).collect()
    }

    #[test]
    fn lex_should_skip_line_comments() {
        assert_eq!(
            kinds("a -- gone\nb", SqlDialect::Postgres),
            [TokenKind::Ident("a"), TokenKind::Ident("b")]
        );
    }

    #[test]
    fn lex_should_skip_block_comments() {
        assert_eq!(
            kinds("a /* x * y */ b", SqlDialect::Sqlite),
            [TokenKind::Ident("a"), TokenKind::Ident("b")]
        );
    }

    #[test]
    fn lex_should_unescape_doubled_quotes_in_quoted_identifier() {
        assert_eq!(
            kinds(r#""a""b""#, SqlDialect::Postgres),
            [TokenKind::Quoted(Cow::Owned("a\"b".to_owned()))]
        );
    }

    #[test]
    fn lex_should_borrow_quoted_identifier_without_escapes() {
        assert!(matches!(
            kinds("`users`", SqlDialect::Mysql).as_slice(),
            [TokenKind::Quoted(Cow::Borrowed("users"))]
        ));
    }

    #[test]
    fn lex_should_unescape_doubled_single_quotes_in_string() {
        assert_eq!(
            kinds("'it''s'", SqlDialect::Postgres),
            [TokenKind::Str(Cow::Owned("it's".to_owned()))]
        );
    }

    #[test]
    fn lex_should_read_postgres_dollar_quoted_body_as_single_token() {
        assert_eq!(
            kinds("$f$ a; b $f$ x", SqlDialect::Postgres),
            [TokenKind::DollarStr(" a; b "), TokenKind::Ident("x")]
        );
    }

    #[test]
    fn lex_should_treat_dollar_placeholder_as_unknown() {
        assert_eq!(
            kinds("$1", SqlDialect::Postgres),
            [TokenKind::Unknown('$'), TokenKind::Number("1")]
        );
    }

    #[test]
    fn lex_should_treat_hash_as_comment_only_for_mysql() {
        assert_eq!(kinds("a # c\nb", SqlDialect::Mysql).len(), 2);
        assert!(kinds("a # c\nb", SqlDialect::Postgres).len() > 2);
    }

    #[test]
    fn lex_should_lex_mysql_conditional_comment_content() {
        assert_eq!(
            kinds("/*!40101 SET x */;", SqlDialect::Mysql),
            [
                TokenKind::Ident("SET"),
                TokenKind::Ident("x"),
                TokenKind::Punct(';')
            ]
        );
    }

    #[test]
    fn lex_should_read_bracket_identifier_only_for_sqlite() {
        assert_eq!(
            kinds("[a b]", SqlDialect::Sqlite),
            [TokenKind::Quoted(Cow::Borrowed("a b"))]
        );
        assert_eq!(
            kinds("int[]", SqlDialect::Postgres),
            [
                TokenKind::Ident("int"),
                TokenKind::Punct('['),
                TokenKind::Punct(']')
            ]
        );
    }

    #[test]
    fn lex_should_report_one_based_line_and_column() {
        let t = lex("a\r\n  bc", SqlDialect::Postgres);
        assert_eq!(t.get(1).map(|t| (t.line, t.col)), Some((2, 3)));
    }

    #[test]
    fn lex_should_not_panic_on_unterminated_string() {
        assert_eq!(kinds("'abc", SqlDialect::Postgres).len(), 1);
    }

    #[test]
    fn lex_should_not_panic_on_unterminated_block_comment() {
        assert_eq!(kinds("a /* abc", SqlDialect::Postgres).len(), 1);
    }

    #[test]
    fn lex_should_not_panic_on_unterminated_dollar_quote() {
        assert_eq!(kinds("$$abc", SqlDialect::Postgres).len(), 1);
    }

    #[test]
    fn lex_should_keep_keyword_matching_case_insensitive() {
        let t = lex("CrEaTe", SqlDialect::Postgres);
        assert!(t.first().is_some_and(|t| t.is_kw("create")));
    }

    #[test]
    fn lex_should_not_treat_quoted_identifier_as_keyword() {
        let t = lex("\"create\"", SqlDialect::Postgres);
        assert!(t.first().is_some_and(|t| !t.is_kw("create")));
    }

    proptest! {
        #[test]
        fn lex_should_never_panic_on_arbitrary_input(s in "\\PC*") {
            for d in ALL {
                let _ = lex(&s, d);
            }
        }

        #[test]
        fn lex_should_never_panic_on_truncated_sql(cut in 0usize..200) {
            let sql = "CREATE TABLE \"a\" (id int, n varchar(3) DEFAULT 'x''y'); /* c */ -- t\n$$ b $$ `q` [r]";
            let end = sql.char_indices().map(|(i, _)| i).nth(cut).unwrap_or(sql.len());
            for d in ALL {
                let _ = lex(sql.get(..end).unwrap_or(""), d);
            }
        }
    }
}
