import { fileURLToPath } from 'node:url';
import { defineConfig } from 'vitest/config';

// O módulo `vscode` só existe dentro do host da extensão; nos testes ele é substituído por um mock leve.
export default defineConfig({
  resolve: {
    alias: {
      vscode: fileURLToPath(new URL('./src/test/vscode-mock.ts', import.meta.url)),
    },
  },
  test: {
    include: ['src/**/*.test.ts'],
    environment: 'node',
  },
});
