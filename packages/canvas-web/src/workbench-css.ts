/**
 * Estilos do Workbench (protótipo "DB Modeling v5"). Cores vêm só das variáveis CSS
 * preenchidas por `applyTheme` a partir de `glass.ts`; aqui só medidas e composição.
 */
import { FONT_FACE_CSS } from './fonts';

export const WORKBENCH_CSS = `${FONT_FACE_CSS}
.wb, .wb * { box-sizing: border-box; }
.wb { --font: 'Geist', system-ui, -apple-system, 'Segoe UI', sans-serif;
  --mono: 'Geist Mono', ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  position: absolute; inset: 0; display: flex; flex-direction: column; overflow: hidden;
  background: var(--chrome); color: var(--text); font: 13px/1.35 var(--font); -webkit-font-smoothing: antialiased;
  user-select: none; -webkit-user-select: none; }
.wb.glass { background: var(--wall); }
.wb.glass .titlebar, .wb.glass .rail, .wb.glass .sidebar, .wb.glass .editor, .wb.glass .surface, .wb.glass .inspector, .wb.glass .status { backdrop-filter: var(--blur); -webkit-backdrop-filter: var(--blur); }
.wb button { font: inherit; color: inherit; background: none; border: 0; cursor: pointer; padding: 0; }
.wb :is(button, input, textarea, [tabindex]):focus-visible:not(:disabled) { outline: 2px solid var(--accent); outline-offset: -2px; }
.wb input, .wb textarea { font: inherit; color: var(--text); }
.wb svg { flex-shrink: 0; }
.wb .sr-only { position: absolute; width: 1px; height: 1px; margin: -1px; padding: 0; overflow: hidden; clip: rect(0 0 0 0); white-space: nowrap; border: 0; }
.wb .ui { zoom: var(--ui, 1); }
.wb .mono { font-family: var(--mono); }
.wb .grow { flex: 1; }
.wb ::-webkit-scrollbar { width: 10px; height: 10px; }
.wb ::-webkit-scrollbar-thumb { background: var(--edge); border-radius: 6px; border: 2px solid transparent; background-clip: padding-box; }
.wb ::-webkit-scrollbar-corner { background: transparent; }

/* Botões de ícone */
.wb .ib { width: 28px; height: 28px; border-radius: 7px; display: inline-flex; align-items: center; justify-content: center;
  color: var(--faint); position: relative; flex: none; }
.wb .ib:hover { background: var(--hover); color: var(--text); }
.wb .ib.on { background: var(--hover); color: var(--text); }
.wb .ib.sm { width: 22px; height: 22px; border-radius: 5px; }
.wb .kbd, .wb kbd { font: 500 10.5px/18px var(--mono); min-width: 18px; height: 20px; padding: 0 5px; text-align: center; border-radius: 5px;
  border: 1px solid var(--edge-strong); background: var(--input); color: var(--muted); display: inline-block; }

/* Barra de título (42px) */
.wb .titlebar { height: 42px; flex: none; display: flex; align-items: center; gap: 6px; padding: 0 10px 0 16px; white-space: nowrap;
  background: var(--chrome); border-bottom: 1px solid var(--edge-soft); }
.wb .titlebar .ib { color: var(--faint); }
.wb .titlebar .ib:hover, .wb .titlebar .ib.on { color: var(--text); }
.wb .tb-sep { width: 1px; height: 18px; background: var(--edge-soft); margin: 0 6px; flex: none; }
.wb .proj-btn { display: flex; align-items: center; gap: 7px; height: 30px; padding: 0 8px 0 5px; border-radius: 8px; cursor: pointer; margin-right: 10px; color: var(--text); }
.wb .proj-btn:hover, .wb .proj-btn.on { background: var(--hover); }
.wb .proj-btn { min-width: 0; }
.wb .proj-btn .pname { min-width: 0; overflow: hidden; text-overflow: ellipsis; font-size: 13px; font-weight: 600; }
.wb .brand { position: relative; width: 20px; height: 20px; border-radius: 6px; background: var(--accent); flex: none;
  box-shadow: inset 0 1px 0 var(--white-30); }
.wb .brand span { position: absolute; background: var(--white); }
.wb .tab { display: flex; align-items: center; gap: 8px; height: 30px; padding: 0 6px 0 10px; border-radius: 8px; font-size: 12.5px;
  color: var(--text); background: var(--sel); box-shadow: var(--sel-shadow); min-width: 0; max-width: 280px; }
.wb .tab span.nm { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.wb .tab .dirty { width: 7px; height: 7px; border-radius: 50%; background: var(--accent); margin: 0 4.5px; flex: none; }
.wb .tab-new { width: 28px; height: 28px; border-radius: 7px; color: var(--faint); }
.wb .search { height: 28px; border-radius: 8px; background: var(--input); border: 1px solid var(--edge-soft); color: var(--faint);
  font-size: 12.5px; align-items: center; gap: 8px; cursor: pointer; }
.wb .search:hover { color: var(--muted); }
.wb .search.wide { display: flex; width: 250px; padding: 0 10px; }
.wb .search.wide .kbd { height: auto; line-height: normal; font-size: 10.5px; padding: 1px 5px; border-radius: 4px; border-color: var(--edge-soft); background: none; color: var(--faint); flex: none; white-space: nowrap; }
.wb .search.wide > span:first-of-type { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.wb .search.narrow { display: none; width: 28px; height: 28px; justify-content: center; border-radius: 7px; color: var(--faint); border: 0; background: transparent; }
.wb .search.narrow:hover { background: var(--hover); color: var(--text); }
@media (max-width: 999px) { .wb .search.wide { display: none; } .wb .search.narrow { display: flex; } }
.wb .tb-views { display: flex; gap: 2px; padding: 2px; border-radius: 8px; background: var(--input); }
.wb .tb-views button { width: 30px; height: 24px; border-radius: 6px; display: flex; align-items: center; justify-content: center; color: var(--faint); }
.wb .tb-views button:hover { color: var(--text); }
.wb .tb-views button.on { background: var(--sel); color: var(--text); box-shadow: var(--sel-shadow); }
.wb .tb-acts { display: flex; gap: 2px; margin-left: 4px; }
.wb .tb-overflow { display: none; }
@media (max-width: 1000px) {
  .wb .titlebar .tb-acts { display: none; }
  .wb .titlebar .tb-overflow { display: inline-flex; }
  .wb .status .hint { display: none; }
}
@media (max-width: 570px) {
  .wb .titlebar .pname, .wb .titlebar .tb-sep { display: none; }
  .wb .titlebar .proj-btn { margin-right: 0; }
  .wb .menus { left: 8px; right: 8px; }
  .wb .menu.app { max-width: calc(100vw - 64px); }
}
.wb .insp-dot { position: absolute; right: 3px; top: 3px; width: 6px; height: 6px; border-radius: 50%; background: var(--accent); }

/* Trilho esquerdo (48px) */
.wb .main { flex: 1; min-height: 0; display: flex; }
.wb .panel-resize { flex: 0 0 5px; background: var(--edge-soft); cursor: col-resize; touch-action: none; }
.wb .panel-resize:hover, .wb .panel-resize:active { background: var(--accent); }
.wb .rail { width: 48px; flex: none; display: flex; flex-direction: column; align-items: center; gap: 4px; padding: 8px 0; background: var(--rail); }
.wb .rail.left { border-right: 1px solid var(--edge-soft); }
.wb .rbtn { position: relative; width: 34px; height: 34px; border-radius: 9px; display: flex; align-items: center; justify-content: center; color: var(--faint); }
.wb .rbtn:hover { background: var(--hover); color: var(--text); }
.wb .rbtn.on { background: var(--hover); color: var(--text); }
.wb .rbtn.on::before { content: ''; position: absolute; left: -7px; top: 9px; width: 2px; height: 16px; border-radius: 2px; background: var(--accent); }
.wb .rbadge { position: absolute; right: 2px; top: 2px; min-width: 14px; height: 14px; padding: 0 3px; border-radius: 7px; background: var(--danger);
  color: var(--white); font: 600 9px/14px var(--mono); text-align: center; }

/* Painéis esquerdos (248px; editor 440px) */
.wb .sidebar { width: 248px; min-width: min(150px, 30%); max-width: 38%; flex: none; display: flex; flex-direction: column; background: var(--sidebar); border-right: 1px solid var(--edge-soft); overflow: hidden; white-space: nowrap; font-size: 13px; }
.wb .phead { height: 38px; flex: none; display: flex; align-items: center; gap: 8px; padding: 0 8px 0 14px; font-size: 12px; font-weight: 600; color: var(--text); }
.wb .pbody { flex: 1; min-height: 0; overflow-y: auto; overflow-x: hidden; display: flex; flex-direction: column; }
.wb .psec { display: flex; flex-direction: column; border-top: 1px solid var(--edge-soft); padding-bottom: 6px; flex: none; }
.wb .pshd { height: 30px; display: flex; align-items: center; gap: 6px; font-size: 11px; font-weight: 600; letter-spacing: .05em; color: var(--muted); padding: 0 12px 0 10px; cursor: pointer; flex: none; }
.wb .pill { padding: 0 6px; border-radius: 9px; background: var(--input); font: 500 11px/17px var(--mono); letter-spacing: 0; color: var(--faint); }
.wb .cnt-mono { font: 11px var(--mono); letter-spacing: 0; color: var(--faint); }
.wb .trow { display: flex; align-items: center; gap: 8px; height: 28px; margin: 0 6px; padding: 0 8px; border-radius: 6px; font-size: 12.5px; color: var(--muted); cursor: pointer; flex: none; }
.wb .trow:hover { background: var(--hover); }
.wb .trow.on { background: var(--sel); box-shadow: var(--sel-shadow); color: var(--text); }
.wb .trow .nm { overflow: hidden; text-overflow: ellipsis; font-family: var(--mono); font-size: 12.5px; }
.wb .trow.grp { color: var(--text); font-weight: 500; padding: 0 8px; }
.wb .trow.grp .nm { font-family: var(--font); font-size: 12.5px; font-weight: 500; }
.wb .trow.sub { padding-left: 26px; }
.wb .trow.rel { height: 28px; cursor: default; }
.wb .trow.rel:hover { background: none; }
.wb .pfoot { display: flex; align-items: center; gap: 8px; padding: 10px 14px; border-top: 1px solid var(--edge-soft); font-size: 12px; color: var(--muted); cursor: pointer; flex: none; }
.wb .pfoot:hover { background: var(--hover); }
.wb .frow { height: 28px; margin: 0 6px; gap: 8px; padding: 0 8px; }
.wb .frow .nm { font-family: var(--mono); font-size: 12.5px; }
.wb .prob { text-align: left; display: flex; gap: 10px; margin: 0 6px; padding: 7px 8px 7px 10px; border-radius: 7px; cursor: pointer; white-space: normal; }
.wb .prob:hover { background: var(--hover); }
.wb .prob .msg { font-size: 12.5px; line-height: 1.35; }
.wb .prob .sub { font: 11px var(--mono); color: var(--faint); margin-top: 3px; }
.wb .pempty { display: flex; align-items: center; gap: 8px; padding: 6px 16px; font-size: 12.5px; color: var(--muted); white-space: normal; }
.wb .editor { width: 440px; min-width: min(170px, 30%); max-width: 47%; flex: none; display: flex; flex-direction: column; background: var(--editor); border-right: 1px solid var(--edge-soft); overflow: hidden; }
.wb .eh { height: 38px; flex: none; display: flex; align-items: center; gap: 8px; padding: 0 8px 0 12px; font: 12px var(--mono); color: var(--faint); border-bottom: 1px solid var(--edge-soft); white-space: nowrap; }
.wb .eh .state { display: flex; align-items: center; gap: 6px; font: 12px var(--font); }
.wb .eh .ib { width: 24px; height: 24px; color: var(--faint); }
.wb .ebody { flex: 1; min-height: 0; position: relative; }
.wb .ebd { position: absolute; inset: 0; overflow: hidden; pointer-events: none; font: 12.5px/22px var(--mono); }
.wb .ebd-in { width: max-content; min-width: 100%; padding: 10px 0; }
.wb .ln { display: flex; height: 22px; white-space: pre; }
.wb .ln.hl { background: color-mix(in srgb, var(--accent) 7%, transparent); box-shadow: inset 2px 0 0 var(--accent); }
.wb .ln .gut { position: sticky; left: 0; z-index: 1; width: 52px; flex: none; display: flex; background: linear-gradient(var(--gutter), var(--gutter)), var(--editor); }
.wb .ln .num { width: 40px; text-align: right; color: var(--lnum); }
.wb .ln.hl .num { color: var(--muted); }
.wb .ln.err .num { color: var(--danger); }
.wb .ln .mark { width: 12px; display: flex; align-items: center; justify-content: center; }
.wb .ln .mark i { width: 5px; height: 5px; border-radius: 50%; background: var(--danger); }
.wb .ln .code { padding-left: 12px; }
.wb .ln .wavy { text-decoration: underline wavy var(--danger); text-underline-offset: 4px; color: var(--text); }
.wb .ln .emsg { margin-left: 18px; padding: 0 6px; align-self: center; line-height: 17px; border-radius: 4px; font: 11.5px/17px var(--font); color: var(--danger); background: color-mix(in srgb, var(--danger) 10%, transparent); }
.wb .t-kw { color: var(--syn-kw); } .wb .t-ty { color: var(--syn-type); } .wb .t-attr { color: var(--syn-attr); }
.wb .t-num { color: var(--syn-num); } .wb .t-cm { color: var(--syn-comment); } .wb .t-punc { color: var(--syn-punct); }
.wb textarea.code { position: absolute; inset: 0; width: 100%; height: 100%; resize: none; border: 0; outline: 0; background: transparent;
  padding: 10px 12px 10px 64px; user-select: text; -webkit-user-select: text; font: 12.5px/22px var(--mono); white-space: pre; overflow: auto; tab-size: 2;
  color: transparent; -webkit-text-fill-color: transparent; caret-color: var(--accent); }
.wb textarea.code::selection { background: color-mix(in srgb, var(--accent) 30%, transparent); }

/* Canvas */
.wb .surface { flex: 1; min-width: 0; position: relative; overflow: hidden; background-color: var(--surface); outline: none; }
.wb .world { position: absolute; left: 0; top: 0; width: 0; height: 0; transform-origin: 0 0; }
.wb .world svg.edges { position: absolute; left: 0; top: 0; width: 1px; height: 1px; overflow: visible; pointer-events: none; }
.wb[data-cursor="grab"] .surface, .wb[data-cursor="grab"] .world * { cursor: grab !important; }
.wb[data-cursor="grabbing"], .wb[data-cursor="grabbing"] * { cursor: grabbing !important; }
.wb[data-cursor="crosshair"], .wb[data-cursor="crosshair"] * { cursor: crosshair !important; }
.wb .world svg.edges .edge-handle { pointer-events: all; cursor: move; }
.wb .world svg.edges path.edge-hit { pointer-events: stroke; cursor: pointer; }
.wb .group { position: absolute; border-radius: 20px; border: 1px solid; }
.wb .group .gl { position: absolute; top: 12px; left: 16px; display: flex; align-items: center; gap: 7px;
  font-size: 12px; font-weight: 600; cursor: move; }
.wb .group .gl .gpill { padding: 0 6px; border-radius: 9px; font: 500 10.5px/16px var(--mono); }
.wb .card { position: absolute; width: 260px; background: var(--card); border: 1px solid var(--edge-soft); border-radius: 12px; box-shadow: var(--card-shadow); }
.wb.translucent .card { background: var(--card-translucent); backdrop-filter: blur(14px) saturate(150%); }
.wb .card.sel { box-shadow: var(--card-shadow-sel); border-color: var(--accent); }
.wb .card .head { height: 44px; display: flex; align-items: center; gap: 9px; padding: 0 12px; border-bottom: 1px solid var(--edge-soft); cursor: move; }
.wb .card .head .nm { flex: 1; min-width: 0; font-weight: 600; font-size: 13px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.wb .card .head .sch { padding: 0 5px; border-radius: 4px; background: var(--input); color: var(--muted); font: 500 10px/16px var(--mono); flex: none; }
.wb .card .head .cnt { font: 10.5px var(--mono); color: var(--faint); flex: none; }
.wb .card .rows { padding: 4px 0; }
.wb .card.enum-card { width: 220px; border-color: var(--edge); }
.wb .card.enum-card .head { height: 40px; padding: 0 12px; background: transparent; }
.wb .card.enum-card .tag { font: 600 9px var(--mono); letter-spacing: .04em; color: var(--faint); }
.wb .card.enum-card .rows { padding: 4px 0 6px; }
.wb .card.enum-card .ev { height: 26px; display: flex; align-items: center; gap: 9px; padding: 0 12px; font: 12px var(--mono); color: var(--muted); }
.wb .card.enum-card .ev .ev-ic { width: 14px; display: flex; justify-content: center; color: var(--accent); flex: none; }
.wb .row { position: relative; height: 30px; padding: 0 12px; display: flex; align-items: center; gap: 8px; font-size: 12.5px; }
.wb .row:hover { background: var(--hover); }
.wb .row .rail14 { width: 14px; display: flex; justify-content: center; flex: none; }
.wb .row .cn { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.wb .row .cn.pk { font-weight: 500; }
.wb .row .ty { font: 11px var(--mono); color: var(--faint); }
.wb .badge { font: 600 9px var(--mono); letter-spacing: .02em; color: var(--faint); }
.wb .badge.idx { color: var(--idx); }
.wb .row .port { position: absolute; right: -6px; top: 9px; width: 12px; height: 12px; border-radius: 99px; border: 2px solid var(--accent);
  background: var(--card); cursor: crosshair; display: none; }
.wb .row:hover .port { display: block; }
.wb .card .plus { position: absolute; left: 119px; bottom: -11px; width: 22px; height: 22px; border-radius: 99px; background: var(--accent);
  color: var(--white); display: none; align-items: center; justify-content: center; cursor: pointer; }
.wb .card:hover .plus { display: flex; }
.wb .marquee { position: absolute; border: 1px solid var(--accent); background: var(--selected); display: none; pointer-events: none; }
.wb .island { position: absolute; z-index: 20; display: flex; align-items: center; gap: 2px; padding: 4px; border-radius: 12px;
  background: var(--island); border: 1px solid var(--edge-soft); box-shadow: var(--island-shadow); backdrop-filter: blur(20px); -webkit-backdrop-filter: blur(20px); }
.wb .island .sep { width: calc(100% - 8px); height: 1px; margin: 3px 4px; background: var(--edge-soft); }
.wb .island .tb { width: 32px; height: 32px; border-radius: 8px; display: inline-flex; align-items: center; justify-content: center; color: var(--muted); }
.wb .island .tb:hover { background: var(--hover); color: var(--text); }
.wb .island .tb.on { background: var(--accent); color: var(--white); }
.wb .island .tb.acc { background: color-mix(in srgb, var(--accent) 16%, transparent); color: var(--accent); }
.wb .tools { left: 12px; top: 50%; transform: translateY(-50%); flex-direction: column; }
.wb .zoomi { right: 12px; bottom: 12px; padding: 3px; border-radius: 10px; font: 12px var(--mono); color: var(--muted); white-space: nowrap; }
.wb .zoomi .zsep { width: 1px; height: 14px; background: var(--edge-soft); flex: none; }
.wb .zoomi .zbtn { padding: 6px 8px; border-radius: 7px; display: inline-flex; align-items: center; justify-content: center; color: var(--muted); }
.wb .zoomi .zbtn:hover { background: var(--hover); color: var(--text); }
.wb .zoomi .zbtn.acc { background: color-mix(in srgb, var(--accent) 16%, transparent); color: var(--accent); }
.wb .zoomi .pct { width: 38px; text-align: center; color: var(--text); font: 12px var(--mono); border-radius: 6px; padding: 4px 0; }
.wb .zoomi .pct:hover { background: var(--hover); }
.wb .legend { position: absolute; z-index: 21; right: 12px; bottom: 56px; width: 256px; padding: 6px; border-radius: 12px; display: none;
  background: var(--pop); border: 1px solid var(--edge-soft); box-shadow: var(--pop-shadow); backdrop-filter: blur(24px); -webkit-backdrop-filter: blur(24px); font-size: 12.5px; }
.wb .minimap { position: absolute; z-index: 20; right: 12px; bottom: 56px; width: 210px; height: 136px; border-radius: 12px; overflow: hidden; display: none; cursor: pointer;
  background: var(--pop); border: 1px solid var(--edge-soft); box-shadow: var(--pop-shadow); backdrop-filter: blur(24px); -webkit-backdrop-filter: blur(24px); user-select: none; }
.wb .minimap svg { display: block; width: 100%; height: 100%; pointer-events: none; }
.wb .legend .lh { height: 30px; display: flex; align-items: center; gap: 6px; padding: 0 4px 0 8px; font-size: 11px; font-weight: 600; letter-spacing: .05em; color: var(--muted); }
.wb .legend .lr { height: 28px; display: flex; align-items: center; gap: 10px; padding: 0 8px; color: var(--muted); }
.wb .legend .lv { width: 30px; display: flex; align-items: center; justify-content: center; }
.wb .legend .ln-s { width: 26px; border-radius: 2px; display: block; }
.wb .empty { position: absolute; z-index: 10; inset: 0; display: none; align-items: center; justify-content: center; pointer-events: none; }
.wb .empty .in { width: 300px; display: flex; flex-direction: column; gap: 14px; text-align: left; pointer-events: auto; }
.wb .empty .etext { display: flex; flex-direction: column; gap: 4px; padding: 0 10px; }
.wb .empty h3 { margin: 0; font-size: 16px; font-weight: 600; letter-spacing: -.01em; color: var(--text); }
.wb .empty p { margin: 0; font-size: 13px; line-height: 1.5; color: var(--muted); }
.wb .empty .acts { width: 100%; display: flex; flex-direction: column; gap: 2px; font-size: 13.5px; }
.wb .empty .eact { display: flex; align-items: center; gap: 10px; padding: 9px 10px; border-radius: 8px; color: var(--text); font-size: 13.5px; text-align: left; width: 100%; }
.wb .empty .eact:hover { background: var(--hover); }
.wb .empty .eact .k { margin-left: auto; font: 11px var(--mono); color: var(--faint); }
.wb .btn { height: 34px; padding: 0 12px; border-radius: 8px; border: 1px solid var(--edge-strong); display: inline-flex; align-items: center; gap: 8px; font-size: 13px; }
.wb .btn:hover { background: var(--hover); }
.wb .btn:disabled { opacity: .5; cursor: default; }
.wb .btn .k { margin-left: auto; font: 11px var(--mono); color: var(--faint); }
.wb .btn.primary { background: var(--accent); color: var(--white); border-color: transparent; font-weight: 500; box-shadow: inset 0 1px 0 var(--white-20); }
.wb .btn.primary:hover { background: color-mix(in srgb, var(--accent) 88%, var(--white)); }
.wb .btn.primary .k { color: var(--white-75); }
.wb .btn.ghost { border-color: transparent; color: var(--muted); }
.wb .btn.sm { height: 28px; border-radius: 7px; font-size: 12.5px; padding: 0 10px; }
.wb .btn.danger { border-color: transparent; color: var(--danger); justify-content: center; }
.wb .btn.danger:hover { background: color-mix(in srgb, var(--danger) 10%, transparent); }

/* Inspector (288px) */
.wb .inspector { width: 288px; min-width: min(190px, 32%); max-width: 36%; flex: none; display: flex; flex-direction: column; background: var(--sidebar); border-left: 1px solid var(--edge-soft); overflow-y: auto; overflow-x: hidden; position: relative; font-size: 13px; white-space: nowrap; }
.wb .ident { display: flex; align-items: center; gap: 10px; padding: 12px 14px; border-top: 1px solid var(--edge-soft); flex: none; }
.wb .ident .tile { width: 30px; height: 30px; border-radius: 8px; display: flex; align-items: center; justify-content: center; flex: none; }
.wb .ident .id-col { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; }
.wb .isec { display: flex; flex-direction: column; gap: 8px; padding: 4px 14px 14px; border-top: 1px solid var(--edge-soft); flex: none; }
.wb .isec.tight { gap: 6px; }
.wb .isec-hd { height: 30px; display: flex; align-items: center; gap: 6px; font-size: 11px; font-weight: 600; letter-spacing: .05em; color: var(--muted); }
.wb .ilabel { display: flex; align-items: center; font-size: 11px; font-weight: 600; letter-spacing: .05em; color: var(--muted); }
.wb .ilabel .cnt-mono { margin-left: auto; }
.wb .ilabel .ib { margin-left: auto; }
.wb .ititle { font-size: 14px; font-weight: 600; }
.wb .isub { font-size: 11.5px; color: var(--faint); overflow: hidden; text-overflow: ellipsis; }
.wb .hint { font-size: 12.5px; line-height: 1.5; color: var(--muted); white-space: normal; }
.wb .hint kbd { height: auto; line-height: normal; font: 11px var(--mono); padding: 1px 4px; border-radius: 4px; background: none; }
.wb .gen-row { display: flex; align-items: center; }
.wb .gen-row .glbl { width: 64px; color: var(--faint); flex: none; }
.wb .grp-sel { flex: 1; min-width: 0; height: 28px; display: flex; align-items: center; gap: 8px; padding: 0 10px; border-radius: 7px; background: var(--input); border: 1px solid var(--edge-soft); font-size: 13px; color: var(--text); text-align: left; }
.wb .grp-sel:hover { border-color: var(--edge-strong); }
.wb .grp-sel .nm { overflow: hidden; text-overflow: ellipsis; }
.wb .grp-empty { flex: 1; min-width: 0; height: 28px; display: flex; align-items: center; gap: 8px; padding: 0 10px; border-radius: 7px; border: 1px dashed var(--edge-strong); color: var(--faint); font-size: 13px; text-align: left; }
.wb .grp-empty:hover { background: var(--hover); color: var(--muted); }
.wb input.fld { width: 100%; height: 26px; padding: 0 8px; border-radius: 6px; border: 1px solid var(--edge-soft); background: var(--input); outline: 0; user-select: text; -webkit-user-select: text; font: 12px var(--mono); color: var(--text); }
.wb input.fld::placeholder { color: var(--faint); }
.wb input.fld:hover { border-color: var(--edge-strong); }
.wb input.fld:focus { border-color: var(--accent); background: var(--input-solid); color: var(--text); box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 20%, transparent); }
.wb input.fld.big { height: 26px; width: calc(100% + 8px); padding: 0 8px; margin-left: -8px; border-radius: 7px; border-color: transparent; background: transparent; font: 600 14px var(--font); color: var(--text); }
.wb input.fld.big:hover { background: var(--input); }
.wb input.fld.big:focus { background: var(--input-solid); border-color: var(--accent); }
.wb input.fld.bare { background: transparent; border-color: transparent; padding: 0; height: 22px; font: 12.5px var(--mono); color: var(--text); }
.wb input.fld.bare:hover { border-color: transparent; }
.wb input.fld.bare:focus { padding: 0 6px; background: var(--input-solid); border-color: var(--accent); }
.wb .sw { display: flex; gap: 6px; align-items: center; }
.wb .sw button { width: 18px; height: 18px; border-radius: 6px; display: flex; align-items: center; justify-content: center; color: var(--faint); }
.wb .sw button.on { box-shadow: 0 0 0 2px var(--swatch-ring), 0 0 0 3.5px var(--ring, currentColor); }
.wb .sw button.none { border: 1px dashed var(--edge-strong); }
.wb .colsec { display: flex; flex-direction: column; gap: 2px; padding: 4px 8px 10px; border-top: 1px solid var(--edge-soft); flex: none; }
.wb .colsec .isec-hd { padding: 0 6px; }
.wb .colcard { display: flex; flex-direction: column; border-radius: 8px; border: 1px solid transparent; }
.wb .colcard.open { background: var(--hover); border-color: var(--edge-soft); }
.wb .col-top { height: 30px; display: flex; align-items: center; gap: 8px; padding: 0 8px; cursor: pointer; }
.wb .col-top:hover { border-radius: 8px; background: var(--hover); }
.wb .colcard.open .col-top:hover { background: transparent; }
.wb .col-top .cnm { flex: 1; min-width: 0; font: 12.5px var(--mono); color: var(--text); overflow: hidden; text-overflow: ellipsis; }
.wb .col-top .nmf { flex: 1; min-width: 0; }
.wb .col-top .cmarks { font: 600 9px var(--mono); color: var(--faint); flex: none; }
.wb .col-top .cty { font: 11px var(--mono); color: var(--muted); flex: none; }
.wb .col-body { display: grid; grid-template-columns: 48px minmax(0, 1fr); gap: 6px 8px; align-items: center; padding: 4px 10px 10px 30px; font-size: 12px; }
.wb .col-body .clbl { color: var(--faint); }
.wb .col-body .tywrap { position: relative; display: flex; align-items: center; }
.wb .col-body .tywrap input.fld { padding-right: 22px; }
.wb .col-body .tywrap .chev { position: absolute; right: 7px; pointer-events: none; color: var(--faint); display: flex; }
.wb .flags { display: flex; gap: 2px; padding: 2px; border-radius: 7px; background: var(--input); }
.wb .flag { flex: 1; height: 22px; border-radius: 5px; display: flex; align-items: center; justify-content: center; font: 600 10px var(--mono); color: var(--faint); }
.wb .flag:hover { color: var(--text); }
.wb .flag.on { background: var(--sel); color: var(--accent); box-shadow: var(--sel-shadow); }
.wb .presets { display: flex; flex-wrap: wrap; gap: 4px; padding: 8px 6px 0; }
.wb .preset { height: 24px; display: flex; align-items: center; gap: 5px; padding: 0 8px; border-radius: 6px; border: 1px solid var(--edge-soft); font-size: 11.5px; color: var(--muted); }
.wb .preset:hover { background: var(--hover); color: var(--text); }
.wb .preset svg { color: var(--faint); }
.wb .ixcard { display: flex; flex-direction: column; gap: 8px; padding: 10px; border-radius: 9px; border: 1px solid var(--edge-soft); background: var(--hover); }
.wb .ixln { display: flex; align-items: center; gap: 4px; flex-wrap: wrap; }
.wb .ixln .lb { width: 40px; font: 11.5px var(--font); color: var(--faint); flex: none; }
.wb .ixln input.fld { flex: 1; height: 26px; background: var(--input-solid); border-color: var(--edge-soft); }
.wb .chip { height: 22px; display: inline-flex; align-items: center; padding: 0 7px; border-radius: 6px; font: 11px var(--mono); color: var(--muted); border: 1px solid var(--edge-strong); }
.wb .chip:hover { border-color: var(--faint); }
.wb .chip.on { background: color-mix(in srgb, var(--accent) 16%, transparent); color: var(--accent); border-color: color-mix(in srgb, var(--accent) 40%, transparent); }
.wb .irel { height: 30px; display: flex; align-items: center; gap: 8px; padding: 0 6px 0 10px; border-radius: 7px; border: 1px solid var(--edge-soft); font: 12px var(--mono); }
.wb .irel .tx { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.wb .irel .ib, .wb .ixln .ib { color: var(--faint); }
.wb .irel .del-fk { color: var(--muted); font: 10.5px var(--font); flex: none; padding: 3px; border-radius: 4px; }
.wb .irel .del-fk:hover, .wb .irel .ib:hover, .wb .ixln .ib:hover { color: var(--danger); }
.wb .ifoot { display: flex; padding: 10px 14px; border-top: 1px solid var(--edge-soft); flex: none; }
.wb .ifoot .btn.danger { flex: 1; height: 30px; border-radius: 7px; font-size: 12.5px; gap: 7px; }
.wb .multi-ic { position: relative; width: 36px; height: 30px; flex: none; }
.wb .multi-ic .back { position: absolute; left: 8px; top: 0; width: 26px; height: 26px; border-radius: 8px; background: var(--input); border: 1px solid var(--edge-soft); }
.wb .multi-ic .front { position: absolute; left: 0; top: 4px; width: 26px; height: 26px; border-radius: 8px; background: color-mix(in srgb, var(--accent) 16%, transparent); display: flex; align-items: center; justify-content: center; }
.wb .mrow { height: 30px; display: flex; align-items: center; gap: 8px; padding: 0 8px; border-radius: 7px; font: 12.5px var(--mono); cursor: pointer; }
.wb .mrow:hover { background: var(--hover); }
.wb .mrow .g { margin-left: auto; font: 11.5px var(--font); color: var(--faint); }
.wb .evrow { display: flex; align-items: center; gap: 4px; }
.wb .kbrow { height: 28px; display: flex; align-items: center; justify-content: space-between; font-size: 13px; color: var(--muted); border-bottom: 1px solid var(--edge-soft); }
.wb .btnrow { display: flex; flex-wrap: wrap; gap: 6px; }
.wb .btnrow .chip { font: 12px var(--font); height: 24px; }

/* Barra de status (26px) */
.wb .status { height: 26px; flex: none; display: flex; align-items: center; gap: 16px; padding: 0 12px; background: var(--rail);
  border-top: 1px solid var(--edge-soft); font: 11.5px var(--mono); color: var(--faint); white-space: nowrap; }
.wb .status .stat { display: flex; align-items: center; gap: 5px; cursor: pointer; }
.wb .status .cnts { display: flex; align-items: center; gap: 5px; }
.wb .status .hint { flex: 1; text-align: center; font-family: var(--font); color: var(--faint); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 11.5px; line-height: inherit; }
.wb .status .theme { display: flex; align-items: center; gap: 5px; cursor: pointer; color: var(--muted); }
.wb .status .theme:hover { color: var(--text); }

/* Sobreposições */
.wb .menu { position: fixed; z-index: 60; border-radius: 11px; padding: 5px; background: var(--modal); border: 1px solid var(--edge-strong);
  box-shadow: var(--pop-shadow); backdrop-filter: blur(30px); -webkit-backdrop-filter: blur(30px); font-size: 13px; }
.wb .menu .lb { height: 28px; display: flex; align-items: center; padding: 0 10px; font-size: 11px; font-weight: 600; letter-spacing: .06em; color: var(--faint); }
.wb .menu .mi { display: flex; align-items: center; gap: 10px; width: 100%; height: 30px; padding: 0 10px; border-radius: 7px; text-align: left; white-space: nowrap; }
.wb .menu .mi:hover, .wb .menu .mi.on { background: var(--hover); }
.wb .menu .mi svg { color: var(--faint); }
.wb .menu .mi .k { margin-left: auto; font: 11px var(--mono); color: var(--faint); }
.wb .image-menu { padding: 8px; }
.wb .image-menu .lb { height: 32px; padding: 0 10px; margin-bottom: 4px; border-bottom: 1px solid var(--edge-soft); }
.wb .image-menu .image-option { display: grid; grid-template-columns: 30px minmax(0, 1fr) auto; min-height: 50px; height: auto; padding: 6px 10px; }
.wb .image-menu .image-icon { display: grid; place-items: center; width: 30px; height: 30px; border-radius: 7px; background: var(--input); }
.wb .image-menu .image-option .two { font-weight: 600; white-space: normal; }
.wb .image-menu .image-option .two small { font-weight: 400; }
.wb .image-menu .image-option .ext { min-width: 44px; text-align: center; margin: 0; }
.wb .menu .mi .two { display: flex; flex-direction: column; gap: 1px; text-align: left; }
.wb .menu .mi .two small { font-size: 11.5px; color: var(--faint); }
.wb .menu .mi .ext { margin-left: auto; font: 500 10.5px var(--mono); padding: 2px 6px; border-radius: 4px; background: var(--input); color: var(--muted); }
.wb .menu hr { border: 0; border-top: 1px solid var(--edge-soft); margin: 5px 6px; }
.wb .menus { position: fixed; z-index: 60; display: flex; align-items: flex-start; gap: 4px; left: 128px; top: 40px; }
.wb .menus .menu { position: static; min-width: 0; }
.wb .menus .menu .mi { white-space: normal; min-height: 30px; height: auto; padding-block: 5px; }
.wb .menu.app { position: absolute; left: 54px; bottom: 34px; width: 310px; padding: 6px; border-radius: 12px; }
.wb .menu.app .blk { display: flex; flex-direction: column; gap: 8px; padding: 8px 8px 12px; border-bottom: 1px solid var(--edge-soft); }
.wb .menu.app .blk:last-child { border-bottom: 0; padding: 10px 0 4px; gap: 2px; }
.wb .menu.app .ilabel .cnt-mono { font-size: 10.5px; }
.wb .seg { display: grid; gap: 3px; padding: 3px; border-radius: 9px; background: var(--input); }
.wb .seg button { height: 28px; display: flex; align-items: center; justify-content: center; gap: 7px; border-radius: 6px; font-size: 12.5px; color: var(--faint); }
.wb .seg button.on { background: var(--sel); color: var(--text); box-shadow: var(--sel-shadow); }
.wb .seg.acc button.on { background: var(--accent); color: var(--white); box-shadow: 0 1px 2px var(--black-20); }
.wb .seg.acc button { color: var(--muted); }
.wb .seg.row { display: inline-flex; }
.wb .seg.row button { height: 26px; padding: 0 10px; border-radius: 6px; }
.wb .seg button:disabled { opacity: .5; cursor: default; }
.wb .tgl { display: flex; align-items: center; gap: 10px; width: 100%; padding: 7px 8px; border-radius: 7px; text-align: left; }
.wb .tgl:hover { background: var(--hover); }
.wb .tgl .tx { display: flex; flex-direction: column; gap: 2px; flex: 1; }
.wb .tgl .tx small { font-size: 11.5px; color: var(--faint); }
.wb .tgl .sw2 { width: 30px; height: 18px; border-radius: 9px; background: var(--edge-strong); position: relative; flex: none; }
.wb .tgl .sw2 i { position: absolute; top: 2px; left: 2px; width: 14px; height: 14px; border-radius: 50%; background: var(--white); box-shadow: 0 1px 2px var(--black-30); }
.wb .tgl.on .sw2 { background: var(--accent); }
.wb .tgl.on .sw2 i { left: 14px; }
.wb .modal-bg { position: absolute; inset: 0; z-index: 70; background: var(--scrim); display: flex; justify-content: center; align-items: flex-start; padding-top: 64px; }
.wb .modal-bg.mid { align-items: center; padding-top: 0; }
.wb .modal-bg.top76 { padding-top: 76px; }
.wb .modal { width: min(600px, 94vw); max-height: 86%; display: flex; flex-direction: column; border-radius: 14px; background: var(--modal);
  border: 1px solid var(--edge-strong); box-shadow: var(--pop-shadow); overflow: hidden; backdrop-filter: blur(30px) saturate(160%); -webkit-backdrop-filter: blur(30px) saturate(160%); }
.wb .modal.w560 { width: min(560px, 94vw); } .wb .modal.w800 { width: min(800px, 94vw); } .wb .modal.w820 { width: min(820px, 94vw); }
.wb .mhd { display: flex; align-items: flex-start; gap: 12px; padding: 16px 12px 0 18px; }
.wb .mhd .tt { font-size: 15px; font-weight: 600; }
.wb .mhd .st { font-size: 12.5px; color: var(--muted); margin-top: 3px; }
.wb .mhd .ib { color: var(--faint); }
.wb .mbar { height: 48px; flex: none; display: flex; align-items: center; gap: 10px; padding: 0 14px; border-bottom: 1px solid var(--edge-soft); font-size: 14px; color: var(--faint); }
.wb .mbar input { flex: 1; height: 100%; border: 0; background: transparent; outline: 0; font-size: 14px; user-select: text; -webkit-user-select: text; }
.wb .mbar .esc { font: 10.5px var(--mono); padding: 2px 6px; border-radius: 4px; border: 1px solid var(--edge-strong); color: var(--faint); }
.wb .modal .list { overflow-y: auto; padding: 6px; display: flex; flex-direction: column; max-height: 420px; }
.wb .modal .grp { height: 28px; flex: none; display: flex; align-items: center; padding: 0 10px; font-size: 11.5px; font-weight: 500; color: var(--faint); }
.wb .modal .it { height: 34px; flex: none; display: flex; align-items: center; gap: 10px; padding: 0 10px; border-radius: 7px; cursor: pointer; font-size: 13px; }
.wb .modal .it svg { color: var(--faint); }
.wb .modal .it .keys { margin-left: auto; display: flex; gap: 3px; flex: none; }
.wb .modal .it.on, .wb .modal .it:hover { background: var(--hover); }
.wb .mfoot { height: 34px; flex: none; display: flex; align-items: center; gap: 16px; padding: 0 14px; border-top: 1px solid var(--edge-soft); font-size: 11.5px; color: var(--faint); }
.wb .shc { display: grid; grid-template-columns: 1fr 1fr; gap: 0 28px; padding: 8px 18px 18px; overflow-y: auto; }
.wb .shc .gt { font-size: 11px; font-weight: 600; letter-spacing: .06em; color: var(--faint); padding: 14px 0 6px; text-transform: uppercase; }
.wb .shc .sr { height: 28px; display: flex; align-items: center; gap: 4px; font-size: 13px; border-bottom: 1px solid var(--edge-soft); }
.wb .shc .sr span:first-child { flex: 1; }
.wb .shd { height: 52px; flex: none; display: flex; align-items: center; gap: 10px; padding: 0 12px 0 18px; border-bottom: 1px solid var(--edge-soft); }
.wb .shd .tt { font-size: 15px; font-weight: 600; }
.wb .shd input { height: 28px; width: 200px; padding: 0 10px; border-radius: 7px; background: var(--input); border: 1px solid var(--edge-soft); outline: 0; font-size: 12px; user-select: text; -webkit-user-select: text; }
.wb .mbody { padding: 14px 18px 6px; display: flex; flex-direction: column; gap: 10px; overflow: auto; }
.wb .mrowh { display: flex; align-items: center; gap: 10px; }
.wb .panes { display: grid; grid-template-columns: 1fr 1fr; gap: 10px; padding: 0 18px; }
.wb .pane { height: 300px; display: flex; flex-direction: column; border-radius: 10px; border: 1px solid var(--edge-strong); background: var(--editor); overflow: hidden; }
.wb .pane .ph { height: 30px; flex: none; display: flex; align-items: center; gap: 7px; padding: 0 10px; border-bottom: 1px solid var(--edge-soft); font-size: 11.5px; color: var(--faint); }
.wb .pane textarea { flex: 1; min-height: 0; resize: none; border: 0; outline: 0; background: transparent; padding: 6px 12px; font: 12px/19px var(--mono); white-space: pre; user-select: text; -webkit-user-select: text; }
.wb .mfooter { display: flex; align-items: center; gap: 8px; padding: 14px 18px 16px; }
.wb .mfooter .sum { display: flex; align-items: center; gap: 6px; font-size: 12.5px; color: var(--muted); min-width: 0; }
.wb .mfooter .btn { height: 32px; }
.wb .mwarn { margin: 8px 18px 0; max-height: 90px; overflow: auto; font-size: 12px; color: var(--warning); }
.wb .mform { display: grid; grid-template-columns: 96px 1fr; gap: 8px 12px; align-items: center; padding: 10px 18px; }
.wb .mform label { font-size: 12.5px; color: var(--muted); }
.wb .mform input[type=text], .wb .mform input[type=password] { height: 32px; padding: 0 10px; border-radius: 7px; background: var(--input-solid); border: 1px solid var(--edge-strong);
  outline: 0; font: 12.5px var(--mono); user-select: text; -webkit-user-select: text; min-width: 0; }
.wb .mform input:focus { border-color: var(--accent); box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 20%, transparent); }
.wb .mform .mpair { display: flex; align-items: center; gap: 8px; min-width: 0; }
.wb .mform .mpair > input, .wb .mform .pwd { flex: 1 1 0; min-width: 0; }
.wb .mform .mpair > input.narrow { flex: 0 0 84px; }
.wb .mform .mpair .inl { flex: none; }
.wb .mform .mcell { display: flex; flex-direction: column; gap: 6px; min-width: 0; }
.wb .mform .mcell .btn { flex: none; height: 32px; }
.wb .mform .ferr { font-size: 12px; color: var(--danger); }
.wb .mform .ferr:empty { display: none; }
.wb .mform input[aria-invalid="true"] { border-color: var(--danger); }
.wb .mform .pwd { position: relative; display: flex; }
.wb .mform .pwd input { flex: 1; padding-right: 34px; }
.wb .mform .pwd .ib { position: absolute; right: 2px; top: 2px; width: 28px; height: 28px; color: var(--muted); }
.wb .mform .pwd .ib:hover { background: var(--hover); color: var(--text); }
.wb .mform .dd { position: relative; }
.wb .mform .dd-trigger { flex: 1; display: flex; align-items: center; justify-content: space-between; gap: 8px; height: 32px; padding: 0 10px; border-radius: 7px;
  background: var(--input-solid); border: 1px solid var(--edge-strong); font-size: 12.5px; text-align: left; }
.wb .mform .dd-trigger:hover { border-color: var(--faint); }
.wb .mform .dd-trigger:disabled { opacity: .5; cursor: default; }
.wb .menu.dd-list { position: absolute; z-index: 5; top: calc(100% + 4px); left: 0; right: 0; width: auto; }
.wb .menu.dd-list[hidden] { display: none; }
.wb .menu.dd-list .mi[aria-selected="true"] { font-weight: 600; }
.wb .menu.dd-list .mi[aria-disabled="true"] { cursor: default; color: var(--faint); }
.wb .menu.dd-list .mi[aria-disabled="true"]:hover { background: none; }
.wb .mform .chk { grid-column: 1 / -1; display: flex; align-items: center; gap: 8px; font-size: 12px; color: var(--muted); }
.wb .mform .tls-row { display: flex; align-items: center; gap: 16px; flex-wrap: wrap; min-height: 28px; }
.wb .mform .sw-wrap, .wb .mform .chk-inl { display: inline-flex; align-items: center; gap: 8px; font-size: 12.5px; color: var(--text); cursor: pointer; user-select: none; }
.wb .mform .chk-inl { color: var(--muted); }
.wb .mform input.sw { appearance: none; -webkit-appearance: none; position: relative; width: 34px; height: 18px; margin: 0; border-radius: 9px; background: var(--edge-strong); border: 0; cursor: pointer; transition: background .15s var(--ease); flex: none; }
.wb .mform input.sw::after { content: ''; position: absolute; top: 2px; left: 2px; width: 14px; height: 14px; border-radius: 50%; background: #fff; transition: transform .15s var(--ease); }
.wb .mform input.sw:checked { background: var(--accent); }
.wb .mform input.sw:checked::after { transform: translateX(16px); }
.wb .mform input.sw:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }
.wb .mform input.sw:disabled { opacity: .5; cursor: default; }
.wb .mnote { display: flex; align-items: flex-start; gap: 8px; margin: 4px 18px 0; padding: 10px 12px; border-radius: 8px; background: var(--input); font-size: 12.5px; color: var(--muted); }
.wb .mnote #db-ico { flex: none; margin-top: 2px; }
.wb .merr-extra { display: flex; flex-direction: column; gap: 8px; margin: 8px 18px 0; }
.wb .merr-actions { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
.wb .merr-detail { margin: 0; padding: 8px 10px; max-height: 88px; overflow: auto; border-radius: 8px; background: var(--input); border: 1px solid var(--edge-strong); font: 11.5px/1.45 var(--mono); color: var(--muted); white-space: pre-wrap; word-break: break-word; user-select: text; -webkit-user-select: text; }
.wb .suggest { position: fixed; z-index: 65; width: 220px; padding: 4px; border-radius: 9px; background: var(--pop); border: 1px solid var(--edge-strong); box-shadow: var(--pop-shadow);
  backdrop-filter: blur(24px); -webkit-backdrop-filter: blur(24px); }
.wb .suggest .hh { display: flex; justify-content: space-between; align-items: center; height: 22px; padding: 0 6px; font-size: 10.5px; color: var(--faint); }
.wb .suggest .op { display: flex; align-items: center; gap: 8px; height: 28px; padding: 0 6px; border-radius: 6px; cursor: pointer; }
.wb .suggest .op:hover { background: var(--hover); }
.wb .suggest .op span { font: 500 12px var(--mono); }
.wb .suggest .op small { margin-left: auto; font-size: 11px; color: var(--faint); }
.wb .toast { position: absolute; z-index: 80; left: 50%; bottom: 44px; transform: translateX(-50%); padding: 8px 14px; border-radius: 8px;
  background: var(--modal); border: 1px solid var(--edge-strong); box-shadow: var(--pop-shadow); color: var(--text); display: none; font-size: 12.5px; }

/* ───────── Tour Guiado (Spotlight) & Modal de Novidades (What's New) ───────── */
.wb .status .ver-btn { background: transparent; border: 0; color: inherit; font: inherit; padding: 0 5px; height: 18px; border-radius: 4px; cursor: pointer; display: inline-flex; align-items: center; }
.wb .status .ver-btn:hover { background: var(--hover); color: var(--text); }
.wb .wb-tour-layer { position: absolute; inset: 0; z-index: 85; pointer-events: auto; }
.wb .wb-tour-scrim { position: absolute; inset: 0; background: var(--scrim); }
.wb .wb-tour-hole { position: absolute; border-radius: 10px; border: 2px solid var(--accent); box-shadow: 0 0 0 9999px var(--scrim), 0 0 0 4px color-mix(in srgb, var(--accent) 26%, transparent); pointer-events: none; transition: left .2s var(--ease), top .2s var(--ease), width .2s var(--ease), height .2s var(--ease); }
.wb .wb-tour-card { position: absolute; width: min(320px, calc(100vw - 24px)); padding: 14px 16px 14px; border-radius: 12px; background: var(--modal); border: 1px solid var(--edge-strong); box-shadow: var(--pop-shadow); color: var(--text); display: flex; flex-direction: column; gap: 8px; transition: left .2s var(--ease), top .2s var(--ease); }
.wb .wb-tour-top { display: flex; align-items: center; justify-content: space-between; gap: 8px; }
.wb .wb-tour-step { font-size: 11px; font-weight: 600; letter-spacing: .04em; text-transform: uppercase; color: var(--accent); }
.wb .wb-tour-title { margin: 0; font-size: 14.5px; font-weight: 600; color: var(--text); }
.wb .wb-tour-body { margin: 0; font-size: 12.5px; line-height: 1.5; color: var(--muted); }
.wb .wb-tour-foot { display: flex; align-items: center; justify-content: space-between; gap: 8px; margin-top: 6px; }
.wb .wb-tour-dots { display: flex; align-items: center; gap: 5px; }
.wb .wb-tour-dot { width: 6px; height: 6px; border-radius: 99px; background: var(--edge-strong); }
.wb .wb-tour-dot.on { width: 16px; background: var(--accent); }
.wb .wb-tour-btns { display: flex; align-items: center; gap: 6px; }
.wb .wb-tour-btns .btn { height: 28px; padding: 0 10px; font-size: 12px; }
.wb .wb-tour-arrow { position: absolute; width: 12px; height: 12px; background: var(--modal); border: 1px solid var(--edge-strong); transform: rotate(45deg); }
.wb .wb-tour-arrow[data-side="top"] { top: -7px; margin-left: -6px; border-right: 0; border-bottom: 0; }
.wb .wb-tour-arrow[data-side="bottom"] { bottom: -7px; margin-left: -6px; border-left: 0; border-top: 0; }
.wb .wb-tour-arrow[data-side="left"] { left: -7px; margin-top: -6px; border-right: 0; border-top: 0; }
.wb .wb-tour-arrow[data-side="right"] { right: -7px; margin-top: -6px; border-left: 0; border-bottom: 0; }
.wb .wn-list { padding: 12px 18px 4px; overflow-y: auto; display: flex; flex-direction: column; gap: 16px; max-height: 440px; }
.wb .wn-rel { display: flex; flex-direction: column; gap: 8px; }
.wb .wn-rel-hd { display: flex; align-items: center; gap: 8px; font-size: 12.5px; font-weight: 600; color: var(--text); }
.wb .wn-ver { font-family: var(--mono); font-size: 11.5px; padding: 2px 7px; border-radius: 6px; background: color-mix(in srgb, var(--accent) 16%, transparent); color: var(--accent); }
.wb .wn-date { font-size: 11.5px; color: var(--faint); font-weight: 400; }
.wb .wn-item { display: flex; align-items: flex-start; gap: 10px; padding: 10px 12px; border-radius: 9px; background: var(--input); border: 1px solid var(--edge-soft); }
.wb .wn-badge { flex: none; font-size: 10.5px; font-weight: 600; padding: 2px 7px; border-radius: 99px; margin-top: 1px; }
.wb .wn-badge.added { background: color-mix(in srgb, var(--ok) 18%, transparent); color: var(--ok); }
.wb .wn-badge.improved { background: color-mix(in srgb, var(--accent) 18%, transparent); color: var(--accent); }
.wb .wn-badge.fixed { background: color-mix(in srgb, var(--warning) 18%, transparent); color: var(--warning); }
.wb .wn-copy { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 3px; }
.wb .wn-copy strong { font-size: 13px; font-weight: 600; color: var(--text); }
.wb .wn-copy span { font-size: 12px; line-height: 1.45; color: var(--muted); }

/* ───────── Movimento: rápido 120ms (hover/press) · base 180ms (menus, seleção) · lento 260ms (painéis, câmera) ───────── */
.wb { --ease: cubic-bezier(.2,.8,.2,1); }
/* estados (hover, foco, ativo) fazem transição em vez de trocar de uma vez */
.wb button, .wb input, .wb textarea, .wb .ib, .wb .tb, .wb .btn, .wb .tab, .wb .trow, .wb .pshd, .wb .pfoot, .wb .prob, .wb .mi, .wb .it,
.wb .op, .wb .mrow, .wb .col-top, .wb .colcard, .wb .search, .wb .proj-btn, .wb .stat, .wb .theme, .wb .card, .wb .card .plus, .wb .sw2, .wb .sw2 i {
  transition: background-color .12s var(--ease), color .12s var(--ease), border-color .12s var(--ease), box-shadow .12s var(--ease),
    opacity .12s var(--ease), transform .12s var(--ease), left .18s var(--ease); }
.wb button:active, .wb .ib:active, .wb .tb:active, .wb .btn:active { transform: scale(.94); }
.wb .card:hover { border-color: var(--edge-strong); }
.wb .card.sel:hover { border-color: var(--accent); }
/* troca de tema: as superfícies fixas fazem crossfade */
.wb, .wb .titlebar, .wb .rail, .wb .sidebar, .wb .editor, .wb .surface, .wb .inspector, .wb .status, .wb .island, .wb .legend {
  transition: background-color .26s var(--ease), color .26s var(--ease), border-color .26s var(--ease); }

/* efeitos reaplicados pelo JS em elementos recriados (--fx-delay negativo retoma do ponto) */
.wb .fx-fade, .wb .fx-in, .wb .fx-pop, .wb .fx-sel, .wb .fx-row, .wb .fx-lift, .wb .fx-drop, .wb .fx-out, .wb .fx-press,
.wb .fx-press .sw2 i, .wb .pshd.fx-press ~ *, .wb .col-top.fx-press ~ .col-body {
  animation-duration: var(--fx-dur, 180ms); animation-delay: var(--fx-delay, 0ms); animation-timing-function: var(--ease); animation-fill-mode: backwards; }
@keyframes fxFade { from { opacity: 0; } }
@keyframes fxIn { from { opacity: 0; transform: translateY(6px); } }
@keyframes fxPop { from { opacity: 0; transform: scale(.9); } }
@keyframes fxSel { from { box-shadow: 0 0 0 10px color-mix(in srgb, var(--accent) 28%, transparent); } }
@keyframes fxRow { from { background-color: transparent; } }
@keyframes fxLift { from { transform: none; box-shadow: var(--card-shadow-sel); } }
@keyframes fxDrop { from { transform: scale(1.012); } }
@keyframes fxOut { to { opacity: 0; transform: scale(.92); } }
@keyframes fxPress { from { transform: scale(.9); } }
@keyframes fxPressRow { from { background-color: var(--active); } }
@keyframes fxKnobOn { from { left: 2px; } }
@keyframes fxKnobOff { from { left: 14px; } }
@keyframes fxSeg { from { background-color: transparent; box-shadow: none; } }
.wb .fx-fade { animation-name: fxFade; }
.wb .fx-in { animation-name: fxIn; }
.wb .fx-pop { animation-name: fxPop; }
.wb .fx-sel { animation-name: fxSel; }
.wb .fx-row { animation-name: fxRow; }
.wb .fx-press { animation-name: fxPress; }
.wb .trow.fx-press, .wb .pshd.fx-press, .wb .pfoot.fx-press, .wb .prob.fx-press, .wb .mi.fx-press, .wb .it.fx-press, .wb .col-top.fx-press,
.wb .tgl.fx-press, .wb .tab.fx-press { animation-name: fxPressRow; }
.wb .seg button.on.fx-press { animation-name: fxSeg; }
.wb .tgl.on.fx-press .sw2 i { animation-name: fxKnobOn; }
.wb .tgl:not(.on).fx-press .sw2 i { animation-name: fxKnobOff; }
/* seção recolhível e coluna do inspector: o conteúdo desce ao abrir */
.wb .pshd.fx-press ~ *, .wb .col-top.fx-press ~ .col-body { animation-name: fxIn; }
/* arrastar tabelas: o cartão "levanta" e assenta ao soltar */
.wb[data-drag="cards"] .card.sel { transform: scale(1.012); box-shadow: var(--card-shadow-sel), 0 18px 40px var(--black-30); z-index: 5; }
.wb .card.fx-lift { animation-name: fxLift; }
.wb .card.fx-drop { animation-name: fxDrop; }
.wb .card.fx-out { animation-name: fxOut; animation-fill-mode: both; pointer-events: none; }

/* sobreposições: entram com fade + escala, saem com fade */
@keyframes fxMenuIn { from { opacity: 0; transform: translateY(-4px) scale(.97); } }
@keyframes fxScrimIn { from { opacity: 0; } }
@keyframes fxModalIn { from { opacity: 0; transform: translateY(10px) scale(.97); } }
@keyframes fxGone { to { opacity: 0; } }
@keyframes fxToastIn { from { opacity: 0; transform: translateX(-50%) translateY(10px); } }
@keyframes fxToastOut { to { opacity: 0; transform: translateX(-50%) translateY(10px); } }
.wb .menu, .wb .suggest { animation: fxMenuIn .14s var(--ease) backwards; transform-origin: top left; }
.wb .menus .menu:nth-child(2) { animation-delay: .04s; }
.wb .menu.noanim { animation: none; }
.wb .modal-bg { animation: fxScrimIn .18s var(--ease) backwards; }
.wb .modal { animation: fxModalIn .18s var(--ease) backwards; }
.wb .menu.out, .wb .menus.out, .wb .modal-bg.out { animation: fxGone .12s var(--ease) forwards; pointer-events: none; }
.wb .menus.out .menu { animation: none; }
.wb .toast { animation: fxToastIn .18s var(--ease) backwards; }
.wb .toast.out { animation: fxToastOut .18s var(--ease) forwards; }

/* Modo Comparar (diff com o git HEAD): verde = adicionado, âmbar = modificado, vermelho = removido */
.wb .card.diff-added { border-color: var(--ok); box-shadow: 0 0 0 1px var(--ok), var(--card-shadow); }
.wb .card.diff-modified { border-color: var(--warning); box-shadow: 0 0 0 1px var(--warning), var(--card-shadow); }
.wb .card.diff-ghost { pointer-events: none; background: color-mix(in srgb, var(--danger) 6%, var(--card)); box-shadow: none; }
.wb .card.diff-ghost .head .nm { text-decoration: line-through; }
.wb .row.diff-added, .wb .card.enum-card .ev.diff-added { background: color-mix(in srgb, var(--ok) 14%, transparent); }
.wb .row.diff-modified, .wb .card.enum-card .ev.diff-modified { background: color-mix(in srgb, var(--warning) 16%, transparent); }
.wb .row.diff-removed, .wb .card.enum-card .ev.diff-removed { background: color-mix(in srgb, var(--danger) 12%, transparent); }
.wb .diff-ghost-row .cn, .wb .card.enum-card .ev.diff-ghost-row { text-decoration: line-through; opacity: .85; }
.wb .dmark { font: 700 12px var(--mono); width: 14px; text-align: center; flex: none; display: inline-block; }
.wb .diff-summary { display: flex; flex-wrap: wrap; gap: 6px; padding: 10px 12px 6px; }
.wb .dchip { font: 600 11px var(--mono); padding: 2px 8px; border-radius: 9px; background: var(--input); white-space: nowrap; }
.wb .diff-row .dmark { margin-right: 2px; }
.wb .diff-note { align-items: flex-start; gap: 8px; padding: 14px 16px; }
.wb .diff-detail { margin-top: 4px; font: 11px var(--mono); color: var(--faint); word-break: break-word; }
.wb .diff-row.fx-press { transform: none; }

@media (prefers-reduced-motion: reduce) {
  .wb, .wb *, .wb *::before, .wb *::after { transition: none !important; animation: none !important; }
}
`;
