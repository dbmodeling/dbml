//! Dados canônicos do tour guiado (*Spotlight Tour*) e do modal de novidades (*What's New*),
//! carregados em tempo de compilação a partir de `assets/product-guide.json`.

use crate::i18n::Lang;
use serde::Deserialize;
use std::sync::OnceLock;

const GUIDE_JSON: &str = include_str!("../../../assets/product-guide.json");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartupGuide {
    None,
    Tour,
    WhatsNew,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LocalizedText {
    pub pt: String,
    pub en: String,
}

impl LocalizedText {
    pub fn pick(&self, lang: Lang) -> &str {
        match lang {
            Lang::PtBr => &self.pt,
            Lang::En => &self.en,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TourTarget {
    TopbarAdd,
    Sidebar,
    Canvas,
    Minimap,
    Inspector,
    ExportBtn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TourPlacement {
    Top,
    Bottom,
    Left,
    Right,
    Center,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TourStep {
    #[expect(
        dead_code,
        reason = "Stable step identifier from canonical product-guide.json"
    )]
    pub id: String,
    pub target: TourTarget,
    pub placement: TourPlacement,
    pub title: LocalizedText,
    pub body: LocalizedText,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HighlightKind {
    Added,
    Improved,
    Fixed,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReleaseHighlight {
    pub kind: HighlightKind,
    pub title: LocalizedText,
    pub detail: LocalizedText,
    #[serde(default)]
    pub action: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReleaseEntry {
    pub version: String,
    pub date: String,
    pub highlights: Vec<ReleaseHighlight>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ProductGuide {
    pub tour: Vec<TourStep>,
    pub releases: Vec<ReleaseEntry>,
}

static GUIDE: OnceLock<ProductGuide> = OnceLock::new();

pub fn product_guide() -> &'static ProductGuide {
    GUIDE.get_or_init(|| serde_json::from_str(GUIDE_JSON).unwrap_or_default())
}

/// Decide se a abertura do app deve exibir o tour (primeira execução), o modal de novidades
/// (após atualização de versão) ou abrir direto no workspace.
pub fn decide_startup_guide(
    current_version: &str,
    onboarding_done: bool,
    last_seen_version: Option<&str>,
) -> StartupGuide {
    if !onboarding_done && last_seen_version.is_none() {
        return StartupGuide::Tour;
    }
    if last_seen_version != Some(current_version) {
        return StartupGuide::WhatsNew;
    }
    StartupGuide::None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_parses_and_top_release_matches_cargo_version() {
        let guide = product_guide();
        assert!(!guide.tour.is_empty(), "tour deve ter ao menos um passo");
        assert!(
            !guide.releases.is_empty(),
            "releases deve ter ao menos uma versão"
        );
        assert_eq!(
            guide.releases[0].version,
            env!("CARGO_PKG_VERSION"),
            "a primeira release em assets/product-guide.json deve acompanhar Cargo.toml"
        );
    }

    #[test]
    fn startup_decision_covers_first_run_upgrade_and_same_version() {
        assert_eq!(
            decide_startup_guide("0.2.0", false, None),
            StartupGuide::Tour
        );
        assert_eq!(
            decide_startup_guide("0.2.0", true, Some("0.1.4")),
            StartupGuide::WhatsNew
        );
        assert_eq!(
            decide_startup_guide("0.2.0", false, Some("0.1.4")),
            StartupGuide::WhatsNew
        );
        assert_eq!(
            decide_startup_guide("0.2.0", true, Some("0.2.0")),
            StartupGuide::None
        );
        assert_eq!(
            decide_startup_guide("0.2.0", false, Some("0.2.0")),
            StartupGuide::None
        );
    }
}
