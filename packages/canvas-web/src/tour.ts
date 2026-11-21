/**
 * Overlay de Tour Guiado (Spotlight Tour) com recorte e seta apontando para o alvo.
 * Compartilhado entre CLI Web e Extensão VS Code via `packages/canvas-web`.
 */

import type { Language } from './i18n';
import { t } from './i18n';
import { iconSvg } from './icons';
import { TourPlacement, TourStep, TourTarget, pickText } from './guide';
import { TOUR_ACTION, TOUR_GEOM } from './constants';

const esc = (s: string): string =>
  String(s).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');

export interface TourCallbacks {
  ensureTarget?: (target: TourTarget) => void;
  onFinish: () => void;
}

interface Box {
  left: number;
  top: number;
  width: number;
  height: number;
}

export interface CardPosition {
  left: number;
  top: number;
  arrowSide: 'top' | 'bottom' | 'left' | 'right' | 'none';
  arrowOffset: number;
}

/**
 * Calcula a posição do card popover e o lado da seta para apontar ao alvo sem sair da viewport.
 */
export function computeTourCardPosition(
  target: Box | null,
  preferred: TourPlacement,
  vpW: number,
  vpH: number,
  cardW: number = TOUR_GEOM.CARD_WIDTH_PX,
  cardH: number = TOUR_GEOM.CARD_HEIGHT_EST_PX,
): CardPosition {
  const clamp = (v: number, lo: number, hi: number) => Math.max(lo, Math.min(Math.max(lo, hi), v));
  const margin = TOUR_GEOM.VIEWPORT_MARGIN_PX;
  const pad = TOUR_GEOM.HOLE_PAD_PX;
  const gap = TOUR_GEOM.CARD_GAP_PX;
  const arrowMin = TOUR_GEOM.ARROW_MIN_OFFSET_PX;
  if (!target || preferred === 'center' || target.width <= 0 || target.height <= 0) {
    return {
      left: clamp(Math.round((vpW - cardW) / 2), margin, vpW - cardW - margin),
      top: clamp(Math.round((vpH - cardH) / 2), margin, vpH - cardH - margin),
      arrowSide: 'none',
      arrowOffset: 0,
    };
  }

  const cx = target.left + target.width / 2;
  const cy = target.top + target.height / 2;

  const fits = (side: 'top' | 'bottom' | 'left' | 'right'): boolean => {
    switch (side) {
      case 'bottom': return target.top + target.height + pad + gap + cardH + margin <= vpH;
      case 'top': return target.top - pad - gap - cardH - margin >= 0;
      case 'right': return target.left + target.width + pad + gap + cardW + margin <= vpW;
      case 'left': return target.left - pad - gap - cardW - margin >= 0;
    }
  };

  const order: Array<'top' | 'bottom' | 'left' | 'right'> =
    preferred === 'bottom' ? ['bottom', 'top', 'right', 'left']
    : preferred === 'top' ? ['top', 'bottom', 'right', 'left']
    : preferred === 'right' ? ['right', 'left', 'bottom', 'top']
    : ['left', 'right', 'bottom', 'top'];

  const side = order.find(fits) ?? order[0];

  if (side === 'bottom' || side === 'top') {
    const left = clamp(Math.round(cx - cardW / 2), margin, vpW - cardW - margin);
    const top =
      side === 'bottom'
        ? clamp(Math.round(target.top + target.height + pad + gap), margin, vpH - cardH - margin)
        : clamp(Math.round(target.top - pad - gap - cardH), margin, vpH - cardH - margin);
    return {
      left,
      top,
      arrowSide: side === 'bottom' ? 'top' : 'bottom',
      arrowOffset: clamp(Math.round(cx - left), arrowMin, cardW - arrowMin),
    };
  }

  const left =
    side === 'right'
      ? clamp(Math.round(target.left + target.width + pad + gap), margin, vpW - cardW - margin)
      : clamp(Math.round(target.left - pad - gap - cardW), margin, vpW - cardW - margin);
  const top = clamp(Math.round(cy - cardH / 2), margin, vpH - cardH - margin);
  return {
    left,
    top,
    arrowSide: side === 'right' ? 'left' : 'right',
    arrowOffset: clamp(Math.round(cy - top), arrowMin, cardH - arrowMin),
  };
}

export class TourController {
  private layer: HTMLElement | null = null;
  private index = 0;
  private readonly abort = new AbortController();

  constructor(
    private readonly wb: HTMLElement,
    private readonly steps: TourStep[],
    private readonly getLang: () => Language,
    private readonly callbacks: TourCallbacks,
  ) {}

  public isOpen(): boolean {
    return this.layer !== null;
  }

  public start(stepIndex = 0): void {
    if (!this.steps.length) return;
    this.index = Math.max(0, Math.min(this.steps.length - 1, stepIndex));
    this.ensureLayer();
    this.renderStep();
  }

  public next(): void {
    if (this.index + 1 >= this.steps.length) {
      this.finish();
    } else {
      this.index += 1;
      this.renderStep();
    }
  }

  public prev(): void {
    if (this.index > 0) {
      this.index -= 1;
      this.renderStep();
    }
  }

  public finish(): void {
    this.close();
    this.callbacks.onFinish();
  }

  public close(): void {
    if (this.layer) {
      this.layer.remove();
      this.layer = null;
    }
  }

  public destroy(): void {
    this.close();
    this.abort.abort();
  }

  private ensureLayer(): void {
    if (this.layer) return;
    const layer = document.createElement('div');
    layer.className = 'wb-tour-layer';
    layer.dataset.tourLayer = '1';
    layer.innerHTML = `<div class="wb-tour-scrim"></div><div class="wb-tour-hole"></div><div class="wb-tour-card" role="dialog" aria-modal="true" aria-labelledby="wb-tour-title" aria-describedby="wb-tour-body"></div>`;
    this.wb.appendChild(layer);
    this.layer = layer;

    window.addEventListener('resize', () => {
      if (this.layer) this.positionCurrent();
    }, { signal: this.abort.signal });

    window.addEventListener('keydown', (e) => {
      if (!this.layer) return;
      if (e.key === 'Escape') {
        e.preventDefault();
        e.stopPropagation();
        this.finish();
      } else if (e.key === 'ArrowRight' || e.key === 'Enter') {
        e.preventDefault();
        e.stopPropagation();
        this.next();
      } else if (e.key === 'ArrowLeft') {
        e.preventDefault();
        e.stopPropagation();
        this.prev();
      }
    }, { capture: true, signal: this.abort.signal });
  }

  private renderStep(): void {
    if (!this.layer) return;
    const step = this.steps[this.index];
    this.callbacks.ensureTarget?.(step.target);

    const lang = this.getLang();
    const isLast = this.index === this.steps.length - 1;
    const card = this.layer.querySelector('.wb-tour-card') as HTMLElement;
    const progress = t('Passo {} de {}', this.index + 1, this.steps.length);
    const dots = this.steps
      .map((_, i) => `<span class="wb-tour-dot ${i === this.index ? 'on' : ''}"></span>`)
      .join('');

    card.innerHTML = `
      <div class="wb-tour-arrow" aria-hidden="true"></div>
      <div class="wb-tour-top">
        <span class="wb-tour-step">${esc(progress)}</span>
        <button type="button" class="ib sm" data-tour-act="${TOUR_ACTION.SKIP}" aria-label="${esc(t('Fechar'))}" title="${esc(t('Pular'))}">${iconSvg('X', 14, 'currentColor')}</button>
      </div>
      <h3 class="wb-tour-title" id="wb-tour-title">${esc(pickText(step.title, lang))}</h3>
      <p class="wb-tour-body" id="wb-tour-body">${esc(pickText(step.body, lang))}</p>
      <div class="wb-tour-foot">
        <div class="wb-tour-dots" aria-hidden="true">${dots}</div>
        <div class="wb-tour-btns">
          <button type="button" class="btn ghost" data-tour-act="${TOUR_ACTION.SKIP}">${esc(t('Pular'))}</button>
          ${this.index > 0 ? `<button type="button" class="btn" data-tour-act="${TOUR_ACTION.PREV}">${esc(t('Anterior'))}</button>` : ''}
          <button type="button" class="btn primary" data-tour-act="${TOUR_ACTION.NEXT}">${esc(t(isLast ? 'Concluir' : 'Próximo'))}</button>
        </div>
      </div>`;

    card.querySelectorAll(`[data-tour-act="${TOUR_ACTION.SKIP}"]`).forEach((b) => b.addEventListener('click', () => this.finish()));
    card.querySelector(`[data-tour-act="${TOUR_ACTION.PREV}"]`)?.addEventListener('click', () => this.prev());
    card.querySelector(`[data-tour-act="${TOUR_ACTION.NEXT}"]`)?.addEventListener('click', () => this.next());

    this.positionCurrent();
    (card.querySelector(`[data-tour-act="${TOUR_ACTION.NEXT}"]`) as HTMLElement | null)?.focus();
  }

  private positionCurrent(): void {
    if (!this.layer) return;
    const step = this.steps[this.index];
    const hole = this.layer.querySelector('.wb-tour-hole') as HTMLElement;
    const scrim = this.layer.querySelector('.wb-tour-scrim') as HTMLElement;
    const card = this.layer.querySelector('.wb-tour-card') as HTMLElement;
    const arrow = this.layer.querySelector('.wb-tour-arrow') as HTMLElement;

    const wbRect = this.wb.getBoundingClientRect();
    const vpW = wbRect.width || window.innerWidth || TOUR_GEOM.FALLBACK_VIEWPORT_W_PX;
    const vpH = wbRect.height || window.innerHeight || TOUR_GEOM.FALLBACK_VIEWPORT_H_PX;

    const targetEl = this.wb.querySelector(`[data-tour="${step.target}"]`) as HTMLElement | null;
    let box: Box | null = null;
    if (targetEl && step.placement !== 'center') {
      const r = targetEl.getBoundingClientRect();
      if (r.width > 0 && r.height > 0) {
        box = {
          left: r.left - wbRect.left,
          top: r.top - wbRect.top,
          width: r.width,
          height: r.height,
        };
      }
    }

    if (box) {
      scrim.style.display = 'none';
      hole.style.display = 'block';
      hole.style.left = `${Math.max(TOUR_GEOM.HOLE_MIN_POS_PX, box.left - TOUR_GEOM.HOLE_PAD_PX)}px`;
      hole.style.top = `${Math.max(TOUR_GEOM.HOLE_MIN_POS_PX, box.top - TOUR_GEOM.HOLE_PAD_PX)}px`;
      hole.style.width = `${box.width + TOUR_GEOM.HOLE_PAD_PX * 2}px`;
      hole.style.height = `${box.height + TOUR_GEOM.HOLE_PAD_PX * 2}px`;
    } else {
      hole.style.display = 'none';
      scrim.style.display = 'block';
    }

    const cardRect = card.getBoundingClientRect();
    const pos = computeTourCardPosition(
      box,
      step.placement,
      vpW,
      vpH,
      cardRect.width || TOUR_GEOM.CARD_WIDTH_PX,
      cardRect.height || TOUR_GEOM.CARD_HEIGHT_EST_PX,
    );
    card.style.left = `${pos.left}px`;
    card.style.top = `${pos.top}px`;

    arrow.dataset.side = pos.arrowSide;
    if (pos.arrowSide === 'none') {
      arrow.style.display = 'none';
    } else {
      arrow.style.display = 'block';
      arrow.style.left = '';
      arrow.style.top = '';
      if (pos.arrowSide === 'top' || pos.arrowSide === 'bottom') {
        arrow.style.left = `${pos.arrowOffset}px`;
      } else {
        arrow.style.top = `${pos.arrowOffset}px`;
      }
    }
  }
}
