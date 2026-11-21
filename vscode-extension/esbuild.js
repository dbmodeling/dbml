import * as esbuild from 'esbuild';
import * as fs from 'fs';
import * as path from 'path';

const isProduction = process.argv.includes('--production');
const isWatch = process.argv.includes('--watch');

async function main() {
  const canvasWebSrc = path.resolve('../packages/canvas-web/dist/index.js');
  const wasmSrc = path.resolve('../packages/canvas-web/src/wasm/dbml_wasm_bg.wasm');
  const wasmJsSrc = path.resolve('../packages/canvas-web/src/wasm/dbml_wasm.js');
  const missing = [canvasWebSrc, wasmSrc, wasmJsSrc].filter((file) => !fs.existsSync(file));
  if (missing.length) {
    throw new Error(`Missing build prerequisites:\n${missing.join('\n')}\nBuild Wasm and canvas-web first (from the repository root: ./scripts/sync-build.sh).`);
  }
  // Garante existência de dist/
  if (!fs.existsSync('dist')) {
    fs.mkdirSync('dist', { recursive: true });
  }

  // Copia o bundle do canvas web para a pasta dist da extensão
  fs.copyFileSync(canvasWebSrc, 'dist/canvas-web.js');
  console.log('✓ Bundle do canvas-web copiado para dist/canvas-web.js');

  // Gera o bundle Wasm embutido
  const wasmBuffer = fs.readFileSync(wasmSrc);
  const wasmBase64 = wasmBuffer.toString('base64');
  let jsCode = fs.readFileSync(wasmJsSrc, 'utf-8');
  jsCode = jsCode.replace(/export\s+\{([^}]+)\};?/g, '');
  jsCode = jsCode.replace(/export\s+function\s+/g, 'function ');
  jsCode = jsCode.replace(/export\s+class\s+/g, 'class ');
  jsCode = jsCode.replace(/\/\* @ts-self-types[^*]*\*\//g, '');
  // `import.meta` é erro de sintaxe em <script> clássico; o Wasm é instanciado via initSync (bytes embutidos)
  jsCode = jsCode.replace(/new URL\('dbml_wasm_bg\.wasm', import\.meta\.url\)/g, 'undefined');

  const wasmBundle = `(function() {
    ${jsCode}
    const base64Data = "${wasmBase64}";
    const binaryString = atob(base64Data);
    const bytes = new Uint8Array(binaryString.length);
    for (let i = 0; i < binaryString.length; i++) {
      bytes[i] = binaryString.charCodeAt(i);
    }
    const __dbmInstance = initSync({ module: bytes });
    window.DbModelingParser = {
      parse_and_analyze: parse_and_analyze,
      apply_command: apply_command,
      format_dbml: format_dbml,
      compute_layout: compute_layout,
      compute_scene: compute_scene,
      Session: Session,
      swap_dbml_lines: swap_dbml_lines,
      export_sql_ddl: export_sql_ddl,
      import_sql_ddl: import_sql_ddl,
      wasmInstance: __dbmInstance
    };
  })();`;
  fs.writeFileSync('dist/dbml-wasm.bundle.js', wasmBundle);
  console.log('✓ Bundle Wasm embutido gerado em dist/dbml-wasm.bundle.js');
  const common = {
    bundle: true,
    format: 'cjs',
    minify: isProduction,
    sourcemap: !isProduction,
    sourcesContent: false,
    external: ['vscode'],
    logLevel: 'info',
  };
  // Extension host Node (desktop/remoto) e extension host web (vscode.dev); o web não inclui child_process.
  const contexts = await Promise.all([
    esbuild.context({ ...common, entryPoints: ['src/extension.ts'], platform: 'node', outfile: 'dist/extension.cjs' }),
    esbuild.context({ ...common, entryPoints: ['src/extension.web.ts'], platform: 'browser', outfile: 'dist/extension.web.js' }),
  ]);

  if (isWatch) {
    await Promise.all(contexts.map((c) => c.watch()));
    console.log('Assistindo alterações em modo watch...');
  } else {
    for (const c of contexts) {
      await c.rebuild();
      await c.dispose();
    }
  }
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
