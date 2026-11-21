#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss
)]
//! Diagram export without UI dependencies: Mermaid `erDiagram`, vector SVG and PNG/JPEG
//! rasterisation (ADR-0007: scene → SVG → raster).

#[cfg(feature = "raster")]
mod constants;
mod docs;
mod mermaid;
#[cfg(feature = "raster")]
mod raster;
mod svg;

pub use docs::{to_html, to_markdown};
pub use mermaid::{CollisionScope, IdentCollision, MermaidExport, to_mermaid, to_mermaid_checked};
#[cfg(feature = "raster")]
pub use raster::{
    MAX_PIXELS, Raster, RasterError, svg_to_jpeg, svg_to_jpeg_fit, svg_to_png, svg_to_png_fit,
};
pub use svg::{Selection, Theme, to_svg, to_svg_with_selection};

/// Estilos de relação por [`edge_key`], como em `edges` do sidecar.
pub type EdgeStyles = std::collections::HashMap<String, EdgeStyle>;

pub use layout::{relation_curve, relation_segments, segments_svg_path};

/// Estilo opcional de uma relação, guardado em `edges` no sidecar de layout (nunca no DBML).
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct EdgeStyle {
    /// Relação reta (`dx`/`dy` ignorados).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub straight: bool,
    /// Deslocamento somado aos dois pontos de controle (mundo).
    #[serde(skip_serializing_if = "is_zero")]
    pub dx: f32,
    /// Deslocamento somado aos dois pontos de controle (mundo).
    #[serde(skip_serializing_if = "is_zero")]
    pub dy: f32,
}

#[allow(clippy::trivially_copy_pass_by_ref)] // assinatura exigida por `skip_serializing_if`
fn is_zero(v: &f32) -> bool {
    *v == 0.0
}

/// Chave da relação no sidecar: `"<tabela>.<cols>-><tabela>.<cols>"`; `from_table`/`to_table`
/// são as chaves de `tables` do sidecar; colunas compostas separadas por vírgula.
#[must_use]
pub fn edge_key(
    from_table: &str,
    from: &schema::Endpoint,
    to_table: &str,
    to: &schema::Endpoint,
) -> String {
    let cols = |e: &schema::Endpoint| {
        if e.columns.is_empty() {
            e.column.clone()
        } else {
            e.columns.join(",")
        }
    };
    format!("{from_table}.{}->{to_table}.{}", cols(from), cols(to))
}

/// Aplica o estilo sobre a curva automática de [`relation_curve`]; `None` devolve a curva intacta.
#[must_use]
pub fn styled_curve(curve: [layout::Pos; 4], style: Option<&EdgeStyle>) -> [layout::Pos; 4] {
    let [p0, mut c1, mut c2, p3] = curve;
    match style {
        None => {}
        Some(s) if s.straight => {
            let at = |k: f32| layout::Pos {
                x: p0.x + (p3.x - p0.x) * k,
                y: p0.y + (p3.y - p0.y) * k,
            };
            (c1, c2) = (at(1.0 / 3.0), at(2.0 / 3.0));
        }
        Some(s) => {
            for c in [&mut c1, &mut c2] {
                c.x += s.dx;
                c.y += s.dy;
            }
        }
    }
    [p0, c1, c2, p3]
}

/// Ponto da cúbica em t=0.5 (posição da alça de edição).
#[must_use]
pub fn curve_mid(c: &[layout::Pos; 4]) -> layout::Pos {
    layout::Pos {
        x: (c[0].x + 3.0 * c[1].x + 3.0 * c[2].x + c[3].x) / 8.0,
        y: (c[0].y + 3.0 * c[1].y + 3.0 * c[2].y + c[3].y) / 8.0,
    }
}

#[cfg(test)]
mod curve_tests {
    use super::*;

    #[test]
    fn portas_e_controles_acompanham_posicao_das_tabelas() {
        let right = relation_curve(0.0, 10.0, 400.0, 80.0);
        assert_eq!(
            (right[0].x, right[1].x, right[2].x, right[3].x),
            (260.0, 330.0, 330.0, 400.0)
        );
        let left = relation_curve(400.0, 10.0, 0.0, 80.0);
        assert_eq!(
            (left[0].x, left[1].x, left[2].x, left[3].x),
            (400.0, 330.0, 330.0, 260.0)
        );
        let overlap = relation_curve(0.0, 10.0, 20.0, 80.0);
        assert_eq!(
            (overlap[0].x, overlap[1].x, overlap[2].x, overlap[3].x),
            (0.0, -70.0, -50.0, 20.0)
        );
        let same_port = relation_curve(0.0, 10.0, 0.0, 10.0);
        assert_eq!(
            (
                same_port[0].y,
                same_port[1].y,
                same_port[2].y,
                same_port[3].y
            ),
            (10.0, 10.0, 18.0, 18.0)
        );
        let self_ref = relation_curve(0.0, 10.0, 0.0, 80.0);
        assert_eq!(
            (self_ref[0].x, self_ref[1].x, self_ref[2].x, self_ref[3].x),
            (0.0, -70.0, -70.0, 0.0)
        );
    }

    #[test]
    fn sem_estilo_e_curva_automatica() {
        let auto = relation_curve(0.0, 10.0, 400.0, 80.0);
        assert_eq!(styled_curve(auto, None), auto);
        assert_eq!(styled_curve(auto, Some(&EdgeStyle::default())), auto);
    }

    #[test]
    fn reta_poe_controles_nos_tercos_e_ignora_deslocamento() {
        // p0 = (260, 10), p3 = (410, 70)
        let auto = relation_curve(0.0, 10.0, 410.0, 70.0);
        let s = EdgeStyle {
            straight: true,
            dx: 99.0,
            dy: 99.0,
        };
        let [p0, c1, c2, p3] = styled_curve(auto, Some(&s));
        assert_eq!((p0, p3), (auto[0], auto[3]));
        let got = [c1.x, c1.y, c2.x, c2.y];
        for (g, w) in got.iter().zip([310.0, 30.0, 360.0, 50.0]) {
            assert!((g - w).abs() < 1e-3, "{got:?}");
        }
    }

    #[test]
    fn deslocamento_soma_aos_dois_controles_e_alca_anda_075() {
        let auto = relation_curve(0.0, 10.0, 400.0, 80.0);
        let s = EdgeStyle {
            straight: false,
            dx: 40.0,
            dy: -24.0,
        };
        let c = styled_curve(auto, Some(&s));
        assert_eq!((c[1].x, c[1].y), (auto[1].x + 40.0, auto[1].y - 24.0));
        assert_eq!((c[2].x, c[2].y), (auto[2].x + 40.0, auto[2].y - 24.0));
        let (m, m0) = (curve_mid(&c), curve_mid(&auto));
        assert_eq!((m.x - m0.x, m.y - m0.y), (30.0, -18.0));
    }

    #[test]
    fn chave_usa_colunas_compostas() {
        let e = |t: &str, cols: &[&str]| schema::Endpoint {
            table: t.into(),
            column: cols[0].into(),
            columns: cols.iter().map(|c| (*c).to_string()).collect(),
            ..Default::default()
        };
        assert_eq!(
            edge_key("a", &e("a", &["x", "y"]), "s.b", &e("b", &["id"])),
            "a.x,y->s.b.id"
        );
    }
}
