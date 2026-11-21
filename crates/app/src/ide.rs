//! Adaptadores do editor: destaque de sintaxe, diagnósticos e autocomplete DBML.
//! A lógica de linguagem vive em `dbml-syntax`/`dbml-analysis`; aqui só há a ponte.

use crate::glass::{Glass, Mode};
use dbml_syntax::{SyntaxKind, parse};
use gpui_kit::component::input::{
    CompletionProvider, EditorState, FoldRange, InputEdit, InputHighlighter, Rope, RopeExt,
};
use gpui_kit::*;
use lsp_types::{
    CompletionContext, CompletionItem, CompletionItemKind, CompletionResponse, CompletionTextEdit,
    TextEdit,
};
use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;

/// Sublinhados de diagnóstico (trecho + cor), compartilhados entre o shell e o highlighter. O
/// shell os atualiza quando a análise em segundo plano termina; o highlighter só os pinta, sem
/// rodar o lint de novo a cada tecla.
pub type DiagMarks = Rc<RefCell<Vec<(Range<usize>, Hsla)>>>;

/// Converte diagnósticos em sublinhados coloridos pela severidade.
pub fn diag_marks(diags: &[dbml_analysis::Diagnostic]) -> Vec<(Range<usize>, Hsla)> {
    diags
        .iter()
        .map(|d| {
            let c = match d.severity {
                dbml_analysis::Severity::Error => crate::glass::DIAG_ERROR,
                dbml_analysis::Severity::Warning => crate::glass::DIAG_WARNING,
                dbml_analysis::Severity::Info => crate::glass::DIAG_INFO,
            };
            (d.range.clone(), rgba(c).into())
        })
        .collect()
}

/// Destaque de DBML a partir do CST (sem tree-sitter). Reanalisa o texto inteiro a cada edição.
#[derive(Default)]
pub struct DbmlHighlighter {
    spans: Vec<(Range<usize>, &'static str)>,
    folds: Vec<Range<usize>>,
    /// Trechos com problema (lint) e a cor do sublinhado, preenchidos pelo shell.
    marks: DiagMarks,
}

const KEYWORDS: [&str; 9] = [
    "table",
    "ref",
    "enum",
    "project",
    "tablegroup",
    "note",
    "indexes",
    "as",
    "records",
];
impl DbmlHighlighter {
    fn rebuild(&mut self, text: &str) {
        self.spans.clear();
        self.folds.clear();
        // os diagnósticos do editor não aparecem pintados nesta versão do gpui-base; o sublinhado
        // vai junto com o destaque de sintaxe, a partir de `marks` (lint roda uma vez, em
        // segundo plano, no shell)
        let root = parse(text).syntax();
        let mut brace_stack = Vec::new();
        let mut in_brackets = false;
        let mut prev_sig: Option<SyntaxKind> = None;
        // fim do último `#`: o valor colado nele é cor hexadecimal
        let mut hash_end: Option<usize> = None;
        // o token anterior foi um nome de atributo (`not null`: os dois são atributo)
        let mut prev_attr = false;
        #[expect(
            clippy::redundant_closure_for_method_calls,
            reason = "rowan is not a direct dependency of app; method call syntax avoids adding it"
        )]
        for tok in root
            .descendants_with_tokens()
            .filter_map(|e| e.into_token())
        {
            let r = usize::from(tok.text_range().start())..usize::from(tok.text_range().end());
            let parent = tok.parent().map(|p| p.kind());
            let lower = tok.text().to_ascii_lowercase();
            let style = match tok.kind() {
                SyntaxKind::Comment => Some("comment"),
                SyntaxKind::Other if tok.text() == "#" => {
                    hash_end = Some(r.end);
                    Some("number")
                }
                SyntaxKind::Ident | SyntaxKind::Number if hash_end == Some(r.start) => {
                    hash_end = Some(r.end);
                    Some("number")
                }
                // conteúdo de `[...]` (settings): nomes dos atributos em destaque próprio
                SyntaxKind::LBracket => {
                    in_brackets = true;
                    Some("punct")
                }
                SyntaxKind::RBracket => {
                    in_brackets = false;
                    Some("punct")
                }
                // nome do atributo: logo após `[` ou `,`, ou colado a outro nome (`not null`);
                // o que vem depois de `:` é valor
                SyntaxKind::Ident
                    if in_brackets
                        && (matches!(prev_sig, Some(SyntaxKind::LBracket | SyntaxKind::Comma))
                            || (prev_sig == Some(SyntaxKind::Ident) && prev_attr)) =>
                {
                    Some("attr")
                }
                SyntaxKind::Str => Some("string"),
                SyntaxKind::Number => Some("number"),
                SyntaxKind::LBrace => {
                    brace_stack.push(r.start);
                    Some("punct")
                }
                SyntaxKind::RBrace => {
                    if let Some(s) = brace_stack.pop() {
                        self.folds.push(s..r.end);
                    }
                    Some("punct")
                }
                SyntaxKind::LParen | SyntaxKind::RParen | SyntaxKind::Comma | SyntaxKind::Colon => {
                    Some("punct")
                }
                SyntaxKind::Other if matches!(tok.text(), ">" | "<" | "-") => Some("punct"),
                SyntaxKind::Ident
                    if KEYWORDS.contains(&lower.as_str())
                        && prev_sig != Some(SyntaxKind::Ident) =>
                {
                    Some("keyword")
                }
                // nome da declaração: identificadores antes da `{` de abertura
                SyntaxKind::Ident
                    if brace_stack.is_empty()
                        && matches!(
                            parent,
                            Some(SyntaxKind::Table | SyntaxKind::Enum | SyntaxKind::TableGroup)
                        ) =>
                {
                    Some("name")
                }
                SyntaxKind::Ident if parent == Some(SyntaxKind::Column) => {
                    // 1º identificador da linha = nome da coluna, o seguinte = tipo
                    if prev_sig.is_none_or(|k| k == SyntaxKind::Newline) {
                        Some("property")
                    } else {
                        Some("type.builtin")
                    }
                }
                _ => None,
            };
            if let Some(s) = style {
                self.spans.push((r, s));
            }
            if tok.kind() == SyntaxKind::Newline {
                prev_sig = None;
            } else if !tok.kind().is_trivia() {
                prev_attr = style == Some("attr");
                prev_sig = Some(tok.kind());
            }
        }
    }
}

impl InputHighlighter for DbmlHighlighter {
    fn language(&self) -> SharedString {
        "dbml".into()
    }

    fn update(
        &mut self,
        _: Option<InputEdit>,
        text: &Rope,
        _: bool,
        _: &mut Window,
        _: &mut Context<EditorState>,
    ) {
        self.rebuild(&text.to_string());
    }

    fn styles(
        &self,
        range: &Range<usize>,
        resolver: &dyn gpui_kit::component::input::HighlightStyleResolver,
    ) -> Vec<(Range<usize>, HighlightStyle)> {
        // fronteiras de todos os trechos (sintaxe e diagnósticos) dentro de `range`
        let marks = self.marks.borrow();
        let mut cuts = vec![range.start, range.end];
        for r in self
            .spans
            .iter()
            .map(|(r, _)| r)
            .chain(marks.iter().map(|(r, _)| r))
        {
            for p in [r.start, r.end] {
                if p > range.start && p < range.end {
                    cuts.push(p);
                }
            }
        }
        cuts.sort_unstable();
        cuts.dedup();
        // o highlighter não vê o modo do app; o resolver segue o tema claro/escuro,
        // então a cor de "property" (texto comum) diz se o fundo é escuro
        let dark = resolver
            .style("property")
            .and_then(|s| s.color)
            .is_none_or(|c| c.l > 0.5);
        let g = Glass::of_scaled(if dark { Mode::Dark } else { Mode::Light }, false, 1.0);
        let own = |name: &str| match name {
            "keyword" => Some(g.syn_kw),
            "name" => Some(g.syn_name),
            "type.builtin" => Some(g.syn_type),
            "attr" => Some(g.syn_attr),
            "number" => Some(g.syn_num),
            "comment" => Some(g.syn_comment),
            "punct" => Some(g.syn_punct),
            _ => None,
        };
        cuts.windows(2)
            .map(|w| {
                let at = w[0];
                let mut style = self
                    .spans
                    .iter()
                    .find(|(r, _)| r.start <= at && at < r.end)
                    .and_then(|(_, name)| {
                        own(name)
                            .map(|c| HighlightStyle {
                                color: Some(c),
                                ..Default::default()
                            })
                            .or_else(|| resolver.style(name))
                    })
                    .unwrap_or_default();
                if let Some((_, color)) =
                    marks.iter().find(|(r, _)| r.start <= at && at < r.end)
                {
                    style.underline = Some(UnderlineStyle {
                        color: Some(*color),
                        thickness: px(1.),
                        wavy: true,
                    });
                }
                (w[0]..w[1], style)
            })
            .collect()
    }

    fn fold_ranges(&self, text: &Rope) -> Vec<FoldRange> {
        self.folds
            .iter()
            .filter_map(|r| {
                let (a, b) = (
                    text.offset_to_point(r.start).row,
                    text.offset_to_point(r.end).row,
                );
                (b > a).then_some(FoldRange {
                    start_line: a,
                    end_line: b,
                })
            })
            .collect()
    }
}

/// Intervalo (em bytes) do bloco `Table <qname> { … }`, sem espaços nas pontas.
pub fn table_block(text: &str, qname: &str) -> Option<Range<usize>> {
    let root = parse(text).syntax();
    #[expect(
        clippy::redundant_closure_for_method_calls,
        reason = "rowan is not a direct dependency of app; method call syntax avoids adding it"
    )]
    let node = root
        .children()
        .filter(|n| n.kind() == SyntaxKind::Table)
        .find(|n| {
            // cabeçalho `Table [schema.]nome [as alias] [opções] {`
            let name: String = n
                .children_with_tokens()
                .filter_map(|e| e.into_token())
                .filter(|t| !t.kind().is_trivia())
                .take_while(|t| !matches!(t.kind(), SyntaxKind::LBrace | SyntaxKind::LBracket))
                .skip(1)
                .take_while(|t| !t.text().eq_ignore_ascii_case("as"))
                .map(|t| dbml_syntax::unquote(t.text()).to_string())
                .collect();
            name == qname
        })?;
    let r = usize::from(node.text_range().start())..usize::from(node.text_range().end());
    let block = &text[r.clone()];
    let start = r.start + (block.len() - block.trim_start().len());
    Some(start..start + block.trim().len())
}

/// Autocomplete DBML.
pub struct DbmlCompletion;

impl CompletionProvider for DbmlCompletion {
    fn completions(
        &self,
        text: &Rope,
        offset: usize,
        _: CompletionContext,
        _: &mut Window,
        _: &mut App,
    ) -> Task<anyhow::Result<CompletionResponse>> {
        let src = text.to_string();
        let items = dbml_analysis::complete(&src, offset)
            .into_iter()
            .map(|c| {
                let kind = match c.kind {
                    dbml_analysis::CompletionKind::Keyword => CompletionItemKind::KEYWORD,
                    dbml_analysis::CompletionKind::Type => CompletionItemKind::TYPE_PARAMETER,
                    dbml_analysis::CompletionKind::Setting => CompletionItemKind::PROPERTY,
                    dbml_analysis::CompletionKind::Table => CompletionItemKind::CLASS,
                    dbml_analysis::CompletionKind::Column => CompletionItemKind::FIELD,
                };
                CompletionItem {
                    label: c.label,
                    kind: Some(kind),
                    detail: Some(c.detail),
                    text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                        range: lsp_types::Range {
                            start: text.offset_to_position(c.replace.start),
                            end: text.offset_to_position(c.replace.end),
                        },
                        new_text: c.insert,
                    })),
                    ..Default::default()
                }
            })
            .collect();
        Task::ready(Ok(CompletionResponse::Array(items)))
    }

    fn is_completion_trigger(&self, _: usize, new_text: &str, _: &mut App) -> bool {
        new_text
            .chars()
            .any(|c| c.is_alphanumeric() || matches!(c, '.' | '[' | ' ' | '>' | ':'))
    }
}

/// Instala destaque e autocomplete DBML num editor. Devolve os sublinhados de diagnóstico, que o
/// chamador atualiza (e depois repinta o editor).
pub fn install(editor: &mut EditorState, cx: &mut Context<EditorState>) -> DiagMarks {
    let marks = DiagMarks::default();
    let shared = marks.clone();
    editor.set_highlighter_factory(
        Rc::new(move |lang: &str| {
            (lang == "dbml").then(|| {
                Box::new(DbmlHighlighter { marks: shared.clone(), ..Default::default() })
                    as Box<dyn InputHighlighter>
            })
        }),
        cx,
    );
    editor.set_highlighter("dbml", cx);
    editor.lsp_mut().completion_provider = Some(Rc::new(DbmlCompletion));
    marks
}

/// Roda o lint do texto do editor (para o painel Problemas e a barra de status).
/// O sublinhado no editor é pintado pelo [`DbmlHighlighter`], não pelo `DiagnosticSet`
/// do gpui-base: nesta versão os diagnósticos dele sobrepõem os estilos com cor
/// transparente e apagam o sublinhado.
pub fn refresh_diagnostics(
    editor: &mut EditorState,
    _: &mut Context<EditorState>,
) -> Vec<dbml_analysis::Diagnostic> {
    dbml_analysis::lint(&editor.value())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[::core::prelude::v1::test]
    fn classifies_names_types_and_settings() {
        let src = "// c\nTable users {\n  id int [pk, default: 1]\n  c int [color: #e56731]\n}\n";
        let mut h = DbmlHighlighter::default();
        h.rebuild(src);
        let kind = |word: &str| {
            let at = src.find(word).unwrap();
            h.spans.iter().find(|(r, _)| r.start == at).map(|(_, k)| *k)
        };
        assert_eq!(kind("Table"), Some("keyword"));
        assert_eq!(kind("users"), Some("name"));
        assert_eq!(kind("int"), Some("type.builtin"));
        assert_eq!(kind("pk"), Some("attr"));
        assert_eq!(kind("// c"), Some("comment"));
        assert_eq!(kind("{"), Some("punct"));
        assert_eq!(kind("["), Some("punct"));
        assert_eq!(kind("1"), Some("number"));
        // cor hexadecimal: `#` e o valor viram número
        assert_eq!(kind("#"), Some("number"));
        assert_eq!(kind("e56731"), Some("number"));
    }

    #[::core::prelude::v1::test]
    fn diag_marks_follow_diagnostic_ranges_and_severity() {
        let diags = dbml_analysis::lint("Table t {\n  a nope_type\n}\nTable t {}\n");
        let marks = diag_marks(&diags);
        assert_eq!(marks.len(), diags.len());
        assert_eq!(marks[0].0, diags[0].range);
    }

    #[::core::prelude::v1::test]
    fn table_block_finds_qualified_and_aliased_tables() {
        let src =
            "// c\nTable users {\n  id int\n}\n\nTable \"app\".\"orders\" as o {\n  id int\n}\n";
        let b = |q| table_block(src, q).map(|r| &src[r]);
        assert_eq!(b("users"), Some("Table users {\n  id int\n}"));
        assert!(b("app.orders").is_some_and(|t| t.ends_with('}')));
        assert_eq!(b("nope"), None);
    }
}
