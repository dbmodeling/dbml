//! Sessão de análise: guarda o documento DBML já analisado por versão de texto.
//!
//! As funções soltas (`parse_and_analyze`, `compute_scene`) reanalisam o texto inteiro a cada
//! chamada. O workbench chamava as duas a cada edição (duas análises) e `compute_scene` a cada
//! `mousemove` durante o arrasto (parse + análise + serialização da cena toda por evento).
//! A [`Session`] analisa **uma vez por versão de texto** e reaproveita o resultado:
//!
//! - [`Session::analyze`] e [`Session::compute_scene`] só reanalisam quando o texto mudou;
//! - [`Session::route_edges`] roteia apenas as arestas com as posições dadas, sem reparse.
//!
//! A lógica vive em tipos puros ([`AnalyzedDocument`], [`SessionCore`]) testáveis sem Wasm; a
//! classe `Session` exportada só converte de/para `JsValue`. As funções soltas continuam
//! existindo por compatibilidade (VS Code embute apenas a lista antiga de exports).

use crate::line_index::LineIndex;
use crate::types::{AnalysisResult, NodeCoordinate, RoutedEdge, SceneGeometry, WasmDiagnostic};
use crate::{
    auto_layout_schema, compute_scene_internal, convert_diagnostic, enum_names,
    route_edges_internal, to_js_json,
};
use schema::Schema;
use std::cell::OnceCell;
use std::collections::HashMap;
use wasm_bindgen::prelude::*;

/// Documento DBML analisado (schema + diagnósticos + enums) para uma versão exata do texto.
#[derive(Debug)]
pub struct AnalyzedDocument {
    source: String,
    schema: Schema,
    diagnostics: Vec<WasmDiagnostic>,
    enums: Vec<String>,
    /// Slots do auto-layout, calculados sob demanda para tabelas sem posição em `route_edges`.
    auto_positions: OnceCell<HashMap<String, NodeCoordinate>>,
}

impl AnalyzedDocument {
    /// Analisa `source` (uma análise + lint; os enums saem do próprio schema).
    #[must_use]
    pub fn analyze(source: &str) -> Self {
        let schema = dbml_analysis::analyze(source);
        let line_index = LineIndex::new(source);
        let diagnostics = dbml_analysis::lint(source)
            .iter()
            .map(|d| convert_diagnostic(d, &line_index, source))
            .collect();
        let enums = enum_names(&schema);
        Self {
            source: source.to_string(),
            schema,
            diagnostics,
            enums,
            auto_positions: OnceCell::new(),
        }
    }

    /// Texto a que esta análise corresponde.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Schema semântico.
    #[must_use]
    pub fn schema(&self) -> &Schema {
        &self.schema
    }

    /// Mesmo resultado de `parse_and_analyze`.
    #[must_use]
    pub fn analysis(&self) -> AnalysisResult {
        AnalysisResult {
            schema: self.schema.clone(),
            diagnostics: self.diagnostics.clone(),
            enums: self.enums.clone(),
        }
    }

    /// Mesmo resultado de `compute_scene`, sem reanalisar o texto.
    #[must_use]
    pub fn scene(&self, provided: Option<&HashMap<String, NodeCoordinate>>) -> SceneGeometry {
        compute_scene_internal(&self.schema, provided)
    }

    /// Roteia só as arestas com `positions` (chamado a cada quadro do arrasto): sem reparse,
    /// sem layout, sem grupos/enums. Tabelas ausentes de `positions` usam o slot do auto-layout.
    #[must_use]
    pub fn route_edges(&self, positions: &HashMap<String, NodeCoordinate>) -> Vec<RoutedEdge> {
        let complete = self
            .schema
            .tables
            .iter()
            .all(|t| positions.contains_key(&t.qualified_name()));
        if complete {
            return route_edges_internal(&self.schema, positions);
        }
        let mut merged = self
            .auto_positions
            .get_or_init(|| auto_layout_schema(&self.schema))
            .clone();
        merged.extend(positions.iter().map(|(k, v)| (k.clone(), *v)));
        route_edges_internal(&self.schema, &merged)
    }
}

/// Cache de uma versão de texto: reanalisa somente quando o texto muda.
#[derive(Debug, Default)]
pub struct SessionCore {
    doc: Option<AnalyzedDocument>,
    analyses: u32,
}

impl SessionCore {
    /// Documento da versão `source`; reaproveita o anterior se o texto for idêntico.
    pub fn update(&mut self, source: &str) -> &AnalyzedDocument {
        if self.doc.as_ref().is_some_and(|d| d.source == source) {
            return self
                .doc
                .get_or_insert_with(|| AnalyzedDocument::analyze(source));
        }
        self.analyses += 1;
        self.doc.insert(AnalyzedDocument::analyze(source))
    }

    /// Último documento analisado, se houver.
    #[must_use]
    pub fn document(&self) -> Option<&AnalyzedDocument> {
        self.doc.as_ref()
    }

    /// Quantas análises completas já foram feitas (cada versão de texto conta uma).
    #[must_use]
    pub fn analysis_count(&self) -> u32 {
        self.analyses
    }
}

/// Lê posições vindas do JS; `undefined`/`null`/inválido viram `None`.
fn positions_from_js(value: &JsValue) -> Option<HashMap<String, NodeCoordinate>> {
    if value.is_undefined() || value.is_null() {
        None
    } else {
        serde_wasm_bindgen::from_value(value.clone()).ok()
    }
}

/// Sessão Wasm: analisa o DBML uma vez por versão de texto e roteia arestas sem reparse.
///
/// O JS deve chamar `free()` quando descartar a sessão (`Workbench.destroy()`).
#[wasm_bindgen]
#[derive(Debug, Default)]
pub struct Session {
    core: SessionCore,
}

#[wasm_bindgen]
impl Session {
    /// Cria uma sessão vazia.
    #[wasm_bindgen(constructor)]
    #[must_use]
    pub fn new() -> Session {
        Session::default()
    }

    /// Mesmo retorno de `parse_and_analyze(source)`; reanalisa só se `source` mudou.
    ///
    /// # Errors
    ///
    /// Returns a [`JsValue`] error if JSON serialization fails.
    pub fn analyze(&mut self, source: &str) -> Result<JsValue, JsValue> {
        to_js_json(&self.core.update(source).analysis())
    }

    /// Mesmo retorno de `compute_scene(source, positions)`, reaproveitando a análise da versão.
    ///
    /// # Errors
    ///
    /// Returns a [`JsValue`] error if JSON serialization fails.
    pub fn compute_scene(
        &mut self,
        source: &str,
        positions_js: &JsValue,
    ) -> Result<JsValue, JsValue> {
        let provided = positions_from_js(positions_js);
        to_js_json(&self.core.update(source).scene(provided.as_ref()))
    }

    /// Arestas roteadas (`RoutedEdge[]`) para `positions`, sem reanalisar. Barato o bastante
    /// para rodar a cada quadro do arrasto. Exige `analyze`/`compute_scene` prévio.
    ///
    /// # Errors
    ///
    /// Returns a [`JsValue`] error if there is no analysed document yet, `positions_js` is not
    /// a `{name: {x, y}}` object, or JSON serialization fails.
    pub fn route_edges(&self, positions_js: &JsValue) -> Result<JsValue, JsValue> {
        let doc = self
            .core
            .document()
            .ok_or_else(|| JsValue::from_str("route_edges: nenhum documento analisado"))?;
        let positions = positions_from_js(positions_js)
            .ok_or_else(|| JsValue::from_str("route_edges: posições inválidas"))?;
        to_js_json(&doc.route_edges(&positions))
    }

    /// Número de análises completas feitas por esta sessão (diagnóstico/testes).
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn analysis_count(&self) -> u32 {
        self.core.analysis_count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = "Table users {\n  id int [pk]\n}\nTable posts {\n  id int [pk]\n  user_id int\n}\nEnum role { admin }\nRef: posts.user_id > users.id\n";

    fn pos(entries: &[(&str, f32, f32)]) -> HashMap<String, NodeCoordinate> {
        entries
            .iter()
            .map(|(k, x, y)| ((*k).to_string(), NodeCoordinate { x: *x, y: *y }))
            .collect()
    }

    #[test]
    fn one_analysis_per_text_version() {
        let mut s = SessionCore::default();
        assert_eq!(s.analysis_count(), 0);
        let _ = s.update(SRC);
        let _ = s.update(SRC);
        let _ = s.update(SRC);
        assert_eq!(s.analysis_count(), 1, "mesmo texto reaproveita a análise");
        let _ = s.update("Table other {\n  id int\n}\n");
        assert_eq!(s.analysis_count(), 2, "texto novo reanalisa uma vez");
    }

    #[test]
    fn analysis_matches_the_free_functions() {
        let doc = AnalyzedDocument::analyze(SRC);
        let a = doc.analysis();
        assert_eq!(a.enums, extract_enums_from_dbml_ref(SRC));
        assert_eq!(a.schema.tables.len(), 2);
        assert_eq!(a.diagnostics.len(), dbml_analysis::lint(SRC).len());
        assert_eq!(doc.source(), SRC);
    }

    fn extract_enums_from_dbml_ref(src: &str) -> Vec<String> {
        crate::extract_enums_from_dbml(src)
    }

    #[test]
    fn scene_equals_compute_scene_internal() {
        let doc = AnalyzedDocument::analyze(SRC);
        let provided = pos(&[("users", 10.0, 20.0), ("posts", 500.0, 40.0)]);
        let a = serde_json::to_value(doc.scene(Some(&provided))).unwrap();
        let schema = dbml_analysis::analyze(SRC);
        let b = serde_json::to_value(compute_scene_internal(&schema, Some(&provided))).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn route_edges_matches_scene_edges_for_complete_positions() {
        let doc = AnalyzedDocument::analyze(SRC);
        let provided = pos(&[("users", 10.0, 20.0), ("posts", 500.0, 40.0)]);
        let scene = doc.scene(Some(&provided));
        let routed = doc.route_edges(&provided);
        assert_eq!(routed.len(), 1);
        assert_eq!(
            serde_json::to_value(&routed).unwrap(),
            serde_json::to_value(&scene.edges).unwrap()
        );
    }

    #[test]
    fn route_edges_follows_new_positions() {
        let doc = AnalyzedDocument::analyze(SRC);
        let left = doc.route_edges(&pos(&[("users", 0.0, 0.0), ("posts", 600.0, 0.0)]));
        let right = doc.route_edges(&pos(&[("users", 600.0, 0.0), ("posts", 0.0, 0.0)]));
        // a aresta nasce em `posts` (origem do Ref) e termina em `users`
        assert!(left[0].ax > left[0].bx, "posts à direita de users");
        assert!(right[0].ax < right[0].bx, "posts à esquerda de users");
    }

    #[test]
    fn route_edges_tolerates_missing_positions() {
        let doc = AnalyzedDocument::analyze(SRC);
        let edges = doc.route_edges(&pos(&[("users", 5.0, 5.0)]));
        assert_eq!(edges.len(), 1);
        let none = doc.route_edges(&HashMap::new());
        assert_eq!(none.len(), 1);
    }

    #[test]
    fn route_edges_does_not_reanalyse() {
        let mut s = SessionCore::default();
        let _ = s.update(SRC);
        let provided = pos(&[("users", 0.0, 0.0), ("posts", 400.0, 0.0)]);
        for i in 0..50 {
            let mut p = provided.clone();
            p.insert(
                "users".into(),
                NodeCoordinate {
                    x: i as f32,
                    y: 0.0,
                },
            );
            let _ = s.document().unwrap().route_edges(&p);
        }
        assert_eq!(s.analysis_count(), 1);
    }

    #[test]
    fn broken_source_keeps_a_usable_document() {
        let mut s = SessionCore::default();
        let doc = s.update("Table {");
        assert_eq!(
            doc.analysis().diagnostics.len(),
            dbml_analysis::lint("Table {").len()
        );
        assert!(doc.source().len() > 3);
        assert!(doc.route_edges(&HashMap::new()).is_empty());
    }
}
