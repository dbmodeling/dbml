import type { CliHost } from './cliHost.js';
import { GIT_BASE_ERROR } from './constants.js';

const REASON = 'Connecting to a database needs the dbml CLI and is not available in VS Code for the Web. Use the desktop version of VS Code.';

/** Host web (vscode.dev / extension host em worker): não há processos nem sockets. */
export const webCliHost: CliHost = {
  unavailableReason: REASON,
  checkCli: () => Promise.reject(new Error(REASON)),
  testConnection: () => Promise.reject(new Error(REASON)),
  introspect: () => Promise.reject(new Error(REASON)),
  readGitHead: () =>
    Promise.resolve({ ok: false, code: GIT_BASE_ERROR.GIT_UNAVAILABLE, message: 'Compare mode needs git and is not available in VS Code for the Web.' }),
};
