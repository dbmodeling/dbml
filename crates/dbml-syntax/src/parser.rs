use crate::{
    DbmlLang, SyntaxKind,
    SyntaxKind::{Block, Column, Ident, LBrace, Newline, RBrace, Ref, Root, Table, TableGroup},
    SyntaxNode, lex,
};
use rowan::GreenNodeBuilder;

/// Result of parsing a DBML source string into an immutable green tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parse {
    green: rowan::GreenNode,
}

impl Parse {
    /// Returns the typed root [`SyntaxNode`] for this parse result.
    #[must_use]
    pub fn syntax(&self) -> SyntaxNode {
        SyntaxNode::new_root(self.green.clone())
    }
}

struct Parser<'a> {
    toks: Vec<(SyntaxKind, &'a str)>,
    pos: usize,
    b: GreenNodeBuilder<'static>,
}

/// Lossless, error-recovering DBML parser that never fails or drops bytes.
#[must_use]
pub fn parse(text: &str) -> Parse {
    let mut p = Parser {
        toks: lex(text),
        pos: 0,
        b: GreenNodeBuilder::new(),
    };
    p.b.start_node(DbmlLang::kind_to_raw(Root));
    while p.pos < p.toks.len() {
        if p.at_kw("table") {
            p.table(Table);
        } else if p.at_kw("tablegroup") {
            p.table(TableGroup);
        } else if p.at_kw("ref") {
            p.ref_line();
        } else {
            p.bump();
        }
    }
    p.b.finish_node();
    Parse {
        green: p.b.finish(),
    }
}

use rowan::Language;

impl Parser<'_> {
    fn kind(&self) -> Option<SyntaxKind> {
        self.toks.get(self.pos).map(|t| t.0)
    }
    fn bump(&mut self) {
        let (k, s) = self.toks[self.pos];
        self.b.token(DbmlLang::kind_to_raw(k), s);
        self.pos += 1;
    }
    fn start(&mut self, k: SyntaxKind) {
        self.b.start_node(DbmlLang::kind_to_raw(k));
    }
    fn at_kw(&self, kw: &str) -> bool {
        matches!(self.toks.get(self.pos), Some((Ident, s)) if s.eq_ignore_ascii_case(kw))
    }

    /// `Ref [nome]: a.b > c.d` até o fim da linha (forma em bloco `Ref { }` fica como tokens soltos).
    fn ref_line(&mut self) {
        self.start(Ref);
        while let Some(k) = self.kind() {
            if k == Newline {
                break;
            }
            self.bump();
        }
        self.b.finish_node();
    }

    /// `Table`/`TableGroup nome [opções] { linhas }`.
    fn table(&mut self, kind: SyntaxKind) {
        self.start(kind);
        // cabeçalho até `{` (ou fim de linha, se malformado)
        while let Some(k) = self.kind() {
            if k == LBrace {
                break;
            }
            if k == Newline {
                self.b.finish_node();
                return;
            }
            self.bump();
        }
        if self.kind().is_none() {
            self.b.finish_node();
            return;
        }
        self.bump(); // {
        loop {
            match self.kind() {
                None => break,
                Some(RBrace) => {
                    self.bump();
                    break;
                }
                Some(k) if k.is_trivia() => self.bump(),
                Some(_) => self.column_or_block(),
            }
        }
        self.b.finish_node();
    }

    /// Uma "linha" dentro da tabela. Se contém `{` no nível 0 (ex.: `indexes {`),
    /// vira `Block` e consome até o `}` correspondente; senão é `Column`.
    fn column_or_block(&mut self) {
        let is_block = {
            let mut i = self.pos;
            let mut found = false;
            while let Some((k, _)) = self.toks.get(i) {
                match k {
                    Newline | RBrace => break,
                    LBrace => {
                        found = true;
                        break;
                    }
                    _ => i += 1,
                }
            }
            found
        };
        self.start(if is_block { Block } else { Column });
        let mut depth = 0usize;
        while let Some(k) = self.kind() {
            match k {
                LBrace => depth += 1,
                RBrace | Newline if depth == 0 => break,
                RBrace => {
                    depth -= 1;
                    if depth == 0 {
                        self.bump();
                        break;
                    }
                    self.bump();
                    continue;
                }
                _ => {}
            }
            self.bump();
        }
        self.b.finish_node();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn lossless_simple() {
        let src = "// c\nTable users {\n  id int [pk] // x\n  indexes {\n    id\n  }\n}\n";
        assert_eq!(parse(src).syntax().text().to_string(), src);
    }

    proptest! {
        #[test]
        fn lossless_any(s in "(?s).{0,200}") {
            prop_assert_eq!(parse(&s).syntax().text().to_string(), s);
        }

        #[test]
        fn lossless_dbmlish(s in "[Table{}\\[\\]a-z \\n/\"]{0,120}") {
            prop_assert_eq!(parse(&s).syntax().text().to_string(), s);
        }
    }
}
