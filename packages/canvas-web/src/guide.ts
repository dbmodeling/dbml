/**
 * Dados canônicos do Tour Guiado (Spotlight Tour) e do Modal de Novidades (What's New).
 * Importa o mesmo `assets/product-guide.json` embutido no app Desktop (`crates/app/src/guide.rs`).
 */

import rawGuide from '../../../assets/product-guide.json';
import type { ActionId } from './actions';
import type { Language } from './i18n';

export type StartupGuide = 'none' | 'tour' | 'whats-new';

export type TourTarget =
  | 'topbar-add'
  | 'sidebar'
  | 'canvas'
  | 'minimap'
  | 'inspector'
  | 'export-btn';

export type TourPlacement = 'top' | 'bottom' | 'left' | 'right' | 'center';

export type HighlightKind = 'added' | 'improved' | 'fixed';

export interface LocalizedText {
  pt: string;
  en: string;
}

export interface TourStep {
  id: string;
  target: TourTarget;
  placement: TourPlacement;
  title: LocalizedText;
  body: LocalizedText;
}

export interface ReleaseHighlight {
  kind: HighlightKind;
  title: LocalizedText;
  detail: LocalizedText;
  action?: ActionId;
}

export interface ReleaseEntry {
  version: string;
  date: string;
  highlights: ReleaseHighlight[];
}

export interface ProductGuide {
  tour: TourStep[];
  releases: ReleaseEntry[];
}

export interface GuideState {
  onboardingDone: boolean;
  lastSeenVersion: string | null;
}

export const PRODUCT_GUIDE: ProductGuide = rawGuide as ProductGuide;

export const APP_VERSION: string = PRODUCT_GUIDE.releases[0]?.version ?? '0.2.0';

export function pickText(text: LocalizedText, lang: Language): string {
  return lang === 'en' ? text.en : text.pt;
}

/**
 * Decide se a abertura do workbench deve exibir o tour (primeira execução),
 * o modal de novidades (após atualização de versão) ou abrir direto no workspace.
 */
export function decideStartupGuide(
  currentVersion: string,
  onboardingDone: boolean,
  lastSeenVersion: string | null | undefined,
): StartupGuide {
  const seen = lastSeenVersion && lastSeenVersion.trim() !== '' ? lastSeenVersion.trim() : null;
  if (!onboardingDone && seen === null) {
    return 'tour';
  }
  if (seen !== currentVersion) {
    return 'whats-new';
  }
  return 'none';
}
