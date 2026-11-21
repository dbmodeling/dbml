#!/usr/bin/env node
/**
 * Measures Wasm analysis/routing speed (#117) and linear memory growth across drag frames (#129).
 * Usage: node scripts/bench-wasm.mjs
 */

import fs from 'node:fs';
import path from 'node:path';
import { performance } from 'node:perf_hooks';
import { fileURLToPath } from 'node:url';

const ROOT_DIR = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const WASM_JS_PATH = path.join(ROOT_DIR, 'packages/canvas-web/src/wasm/dbml_wasm.js');
const WASM_BIN_PATH = path.join(ROOT_DIR, 'packages/canvas-web/src/wasm/dbml_wasm_bg.wasm');

const TABLE_COUNTS = [50, 200, 500];
const DRAG_TABLE_COUNT = 200;
const DRAG_FRAMES = 200;
const EXTRA_COLS = 4;
const GROUP_SIZE = 10;
const DRAG_DELTA_PX = 2;
const BYTES_PER_MIB = 1024 * 1024;

function generateSchema(nTables) {
  const lines = ['Enum status {', '  draft', '  active', '  archived', '}', ''];
  for (let i = 0; i < nTables; i++) {
    lines.push(`Table t_${i} {`);
    lines.push('  id int [pk]');
    if (i > 0) lines.push(`  parent_id int [ref: > t_${i - 1}.id]`);
    for (let c = 0; c < EXTRA_COLS; c++) lines.push(`  col_${c} varchar`);
    lines.push('  state status');
    lines.push('}', '');
  }
  for (let g = 0; g < Math.floor(nTables / GROUP_SIZE); g++) {
    lines.push(`TableGroup grp_${g} {`);
    for (let i = g * GROUP_SIZE; i < (g + 1) * GROUP_SIZE; i++) lines.push(`  t_${i}`);
    lines.push('}', '');
  }
  return lines.join('\n');
}

const wasmMod = await import(WASM_JS_PATH);
const wasmBytes = fs.readFileSync(WASM_BIN_PATH);
const wasmExports = wasmMod.initSync({ module: wasmBytes });
const memMiB = () => (wasmExports.memory.buffer.byteLength / BYTES_PER_MIB).toFixed(2);

console.log(`wasm_size_bytes=${wasmBytes.byteLength}`);
console.log(`mem_init_mib=${memMiB()}`);

for (const n of TABLE_COUNTS) {
  const src = generateSchema(n);
  const session = new wasmMod.Session();
  const t0 = performance.now();
  session.analyze(src);
  const t1 = performance.now();
  const scene = session.compute_scene(src, {});
  const t2 = performance.now();
  session.route_edges(scene.positions);
  const t3 = performance.now();
  session.free();
  console.log(
    `tables=${n} analyze_ms=${(t1 - t0).toFixed(2)} scene_ms=${(t2 - t1).toFixed(2)} route_ms=${(t3 - t2).toFixed(2)} mem_mib=${memMiB()}`
  );
}

// Measure linear memory stability over repeated drag frames on a 200-table schema (#129).
const dragSrc = generateSchema(DRAG_TABLE_COUNT);
const dragSession = new wasmMod.Session();
dragSession.analyze(dragSrc);
const dragScene = dragSession.compute_scene(dragSrc, {});
const positions = { ...dragScene.positions };
const memAfterInit = memMiB();
const tDrag0 = performance.now();
for (let f = 0; f < DRAG_FRAMES; f++) {
  positions.t_0 = { x: positions.t_0.x + DRAG_DELTA_PX, y: positions.t_0.y + DRAG_DELTA_PX };
  dragSession.route_edges(positions);
}
const dragMs = (performance.now() - tDrag0) / DRAG_FRAMES;
const memAfterDrag = memMiB();
dragSession.free();
console.log(
  `drag_frames=${DRAG_FRAMES} avg_route_ms=${dragMs.toFixed(2)} mem_before_mib=${memAfterInit} mem_after_mib=${memAfterDrag}`
);
