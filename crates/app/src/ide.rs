//! Adaptadores do editor: destaque de sintaxe, diagnósticos e autocomplete DBML.
//! A lógica de linguagem vive em `dbml-syntax`/`dbml-analysis`; aqui só há a ponte.

use dbml_syntax::{SyntaxKind, parse};
use gpui_kit::component::input::{
    CompletionProvider, EditorState, FoldRange, InputEdit, InputHighlighter, Rope, RopeExt,
};
use gpui_kit::*;
use lsp_types::{
    CompletionContext, CompletionItem, CompletionItemKind, CompletionResponse, CompletionTextEdit,
    TextEdit,
};
use std::ops::Range;
use std::rc::Rc;

/// Destaque de DBML a partir do CST (sem tree-sitter). Reanalisa o texto inteiro a cada edição.
#[derive(Default)]
pub struct DbmlHighlighter {
    spans: Vec<(Range<usize>, &'static str)>,
    folds: Vec<Range<usize>>,
    /// Trechos com problema (lint) e a cor do sublinhado.
    diags: Vec<(Range<usize>, Hsla)>,
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
const SETTINGS: [&str; 10] = [
    "pk",
    "primary",
    "key",
    "unique",
    "not",
    "null",
    "increment",
    "default",
    "ref",
    "note",
];

impl DbmlHighlighter {
    fn rebuild(&mut self, text: &str) {
        self.spans.clear();
        self.folds.clear();
        // os diagnósticos do editor não aparecem pintados nesta versão do gpui-base;
        // o sublinhado vai junto com o destaque de sintaxe
        self.diags = dbml_analysis::lint(text)
            .into_iter()
            .map(|d| {
                let c = match d.severity {
                    dbml_analysis::Severity::Error => crate::glass::DIAG_ERROR,
                    dbml_analysis::Severity::Warning => crate::glass::DIAG_WARNING,
                    dbml_analysis::Severity::Info => crate::glass::DIAG_INFO,
                };
                (d.range, rgba(c).into())
            })
            .collect();
        let root = parse(text).syntax();
        let mut brace_stack = Vec::new();
        let mut in_brackets = false;
        let mut prev_sig: Option<SyntaxKind> = None;
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
                SyntaxKind::Str => Some("string"),
                SyntaxKind::Number => Some("number"),
                SyntaxKind::LBracket => {
                    in_brackets = true;
                    Some("punctuation.bracket")
                }
                SyntaxKind::RBracket => {
                    in_brackets = false;
                    Some("punctuation.bracket")
                }
                SyntaxKind::LBrace => {
                    brace_stack.push(r.start);
                    Some("punctuation.bracket")
                }
                SyntaxKind::RBrace => {
                    if let Some(s) = brace_stack.pop() {
                        self.folds.push(s..r.end);
                    }
                    Some("punctuation.bracket")
                }
                SyntaxKind::Other if matches!(tok.text(), ">" | "<" | "-") => Some("operator"),
                SyntaxKind::Ident if in_brackets && SETTINGS.contains(&lower.as_str()) => {
                    Some("keyword")
                }
                SyntaxKind::Ident
                    if KEYWORDS.contains(&lower.as_str())
                        && prev_sig != Some(SyntaxKind::Ident) =>
                {
                    Some("keyword")
                }
                SyntaxKind::Ident if parent == Some(SyntaxKind::Table) => Some("type"),
                SyntaxKind::Ident if parent == Some(SyntaxKind::Column) && !in_brackets => {
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
        let mut cuts = vec![range.start, range.end];
        for r in self
            .spans
            .iter()
            .map(|(r, _)| r)
            .chain(self.diags.iter().map(|(r, _)| r))
        {
            for p in [r.start, r.end] {
                if p > range.start && p < range.end {
                    cuts.push(p);
                }
            }
        }
        cuts.sort_unstable();
        cuts.dedup();
        cuts.windows(2)
            .map(|w| {
                let at = w[0];
                let mut style = self
                    .spans
                    .iter()
                    .find(|(r, _)| r.start <= at && at < r.end)
                    .and_then(|(_, name)| resolver.style(name))
                    .unwrap_or_default();
                if let Some((_, color)) =
                    self.diags.iter().find(|(r, _)| r.start <= at && at < r.end)
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

/// Instala destaque e autocomplete DBML num editor.
pub fn install(editor: &mut EditorState, cx: &mut Context<EditorState>) {
    editor.set_highlighter_factory(
        Rc::new(|lang: &str| {
            (lang == "dbml")
                .then(|| Box::new(DbmlHighlighter::default()) as Box<dyn InputHighlighter>)
        }),
        cx,
    );
    editor.set_highlighter("dbml", cx);
    editor.lsp_mut().completion_provider = Some(Rc::new(DbmlCompletion));
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
