import type { CliHost } from './cliHost.js';

const REASON = 'Connecting to a database needs the dbml CLI and is not available in VS Code for the Web. Use the desktop version of VS Code.';

/** Host web (vscode.dev / extension host em worker): não há processos nem sockets. */
export const webCliHost: CliHost = {
  unavailableReason: REASON,
  checkCli: () => Promise.reject(new Error(REASON)),
  testConnection: () => Promise.reject(new Error(REASON)),
  introspect: () => Promise.reject(new Error(REASON)),
};
