import type { CliHost } from './cliHost.js';
import { checkCli, runIntrospect, runTestConnection } from './introspect.js';

/** Host Node: executa a CLI instalada na máquina. */
export const nodeCliHost: CliHost = {
  checkCli,
  testConnection: runTestConnection,
  introspect: runIntrospect,
};
