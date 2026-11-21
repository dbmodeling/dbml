/**
 * Movimento do Workbench: durações e curva únicas (as mesmas do desktop, `crates/app/src/motion.rs`).
 * Manipulação direta (arrastar, roda do mouse, digitar) nunca é animada.
 */
export const MOTION = { fast: 120, base: 180, slow: 260 } as const;

/** Curva ease-out cúbica, k em 0..1. */
export const easeOut = (k: number): number => 1 - Math.pow(1 - Math.min(1, Math.max(0, k)), 3);

export const lerp = (a: number, b: number, k: number): number => a + (b - a) * k;

/** Respeita "reduzir movimento" do sistema: tudo vira instantâneo. */
export const noMotion = (): boolean =>
  typeof matchMedia === 'function' && matchMedia('(prefers-reduced-motion: reduce)').matches;

/** Seletor CSS do elemento que disparou uma ação: `data-act` + os demais `data-*` que o distinguem. */
/** Escapa um valor para uso dentro de `[attr="…"]`. */
export const cssQ = (v: string): string => v.replace(/["\\]/g, '\\$&');

export function actSelector(act: string, data: Record<string, string | undefined> = {}): string {
  const q = cssQ;
  let s = `[data-act="${q(act)}"]`;
  for (const [k, v] of Object.entries(data)) {
    if (v === undefined || k === 'act' || k === 'keep' || k === 'orig' || k === 'on') continue;
    s += `[data-${k.replace(/[A-Z]/g, (c) => '-' + c.toLowerCase())}="${q(v)}"]`;
  }
  return s;
}
