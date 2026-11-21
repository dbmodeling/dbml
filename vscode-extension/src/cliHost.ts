import type { IntrospectRequest, IntrospectResult, TestConnectionResult } from './introspect.js';

/**
 * Fronteira com a execução da CLI `dbml`. A implementação Node usa `child_process`
 * (`cliHost.node.ts`); o build web não pode importá-lo, então recebe uma versão que só recusa.
 */
export interface CliHost {
  /** Preenchido quando a CLI não pode rodar neste host (ex.: extension host web). */
  readonly unavailableReason?: string;
  /** Confere presença e versão; devolve a versão encontrada. */
  checkCli(cliPath: string): Promise<string>;
  testConnection(cliPath: string, req: IntrospectRequest): Promise<TestConnectionResult>;
  introspect(cliPath: string, req: IntrospectRequest): Promise<IntrospectResult>;
}
