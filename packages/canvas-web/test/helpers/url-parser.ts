import type { UrlParser } from '../../src/connect-db';
import { PASSWORD_MASK } from '../../src/constants';
import type { ConnectionUrlError, ParsedConnectionUrl } from '../../src/types';

const SCHEMES: Record<string, ParsedConnectionUrl['dialect']> = {
  'postgres:': 'postgres',
  'postgresql:': 'postgres',
  'mysql:': 'mysql',
  'mariadb:': 'mysql',
  'sqlserver:': 'sqlserver',
  'mssql:': 'sqlserver',
};

/** Parser de teste: espelha só o contrato de `parse_connection_url` (o núcleo real vive no Wasm). */
export const fakeParser: UrlParser = (input) => {
  const fail = (code: ConnectionUrlError['code']): never => {
    throw { code, message: code } satisfies ConnectionUrlError;
  };
  const text = input.trim();
  if (!text) return fail('empty');
  if (!text.includes('://')) {
    return {
      dialect: 'sqlite', host: '', port: null, database: '', user: '', password: '', path: text,
      trust_server_certificate: false, tls: 'auto', schemas: [], password_masked: false,
    };
  }
  let url: URL;
  try {
    url = new URL(text);
  } catch {
    return fail('malformed');
  }
  const dialect = SCHEMES[url.protocol];
  if (!dialect) return fail('unknown_scheme');
  if (!url.hostname) return fail('missing_host');
  const rawPassword = decodeURIComponent(url.password);
  const ssl = url.searchParams.get('sslmode');
  return {
    dialect,
    host: url.hostname,
    port: url.port ? Number(url.port) : null,
    database: decodeURIComponent(url.pathname.replace(/^\//, '')),
    user: decodeURIComponent(url.username),
    password: rawPassword === PASSWORD_MASK ? '' : rawPassword,
    path: '',
    trust_server_certificate: url.searchParams.get('trustServerCertificate') === 'true',
    tls: ssl === 'require' ? 'require' : 'auto',
    schemas: [],
    password_masked: rawPassword === PASSWORD_MASK,
  };
};
