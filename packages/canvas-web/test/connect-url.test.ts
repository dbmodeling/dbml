import { describe, expect, it } from 'vitest';
import {
  ConnectDbModel,
  buildDisplayUrl,
  nextEnabledOption,
  urlErrorMessage,
  type ConnectFields,
  type UrlParser,
} from '../src/connect-db';
import { CONNECT_URL_ERROR, EDIT_ORIGIN, PASSWORD_MASK, TLS_MODE } from '../src/constants';
import { setLanguage } from '../src/i18n';
import type { ParsedConnectionUrl } from '../src/types';
import { fakeParser } from './helpers/url-parser';

const fields = (over: Partial<ConnectFields> = {}): ConnectFields => ({
  host: 'h', port: '5433', database: 'db', user: 'u', password: 'p@ss', path: '', schemas: '', ...over,
});

const filled = (parser: UrlParser = fakeParser): ConnectDbModel => {
  const m = new ConnectDbModel('postgres', parser);
  for (const [name, value] of Object.entries(fields())) m.setField(name as keyof ConnectFields, value);
  return m;
};

describe('buildDisplayUrl (espelho de ConnectionParams::to_url)', () => {
  // Mesmos exemplos fixos dos testes de `crates/db-introspect/src/params.rs`; a porta vem digitada.
  const secret = { user: 'app', password: 'p@ss:w/rd&1', database: 'shop' };

  it('percent-encoda usuário, senha e banco como o núcleo', () => {
    expect(buildDisplayUrl('postgres', fields({ ...secret, host: 'localhost', port: '5432' }), TLS_MODE.AUTO, false, false)).toBe(
      'postgres://app:p%40ss%3Aw%2Frd%261@localhost:5432/shop'
    );
    expect(buildDisplayUrl('mysql', fields({ ...secret, host: 'db.internal', port: '3307' }), TLS_MODE.AUTO, false, false)).toBe(
      'mysql://app:p%40ss%3Aw%2Frd%261@db.internal:3307/shop'
    );
  });

  it('codifica também os caracteres que encodeURIComponent deixa passar', () => {
    const out = buildDisplayUrl('postgres', fields({ user: "a'b", password: '!*()' }), TLS_MODE.AUTO, false, false);
    expect(out).toContain("a%27b:%21%2A%28%29@");
  });

  it('omite a porta quando o campo está vazio e ignora valores inválidos', () => {
    expect(buildDisplayUrl('postgres', fields({ port: '' }), TLS_MODE.AUTO, false, false)).toBe('postgres://u:p%40ss@h/db');
    expect(buildDisplayUrl('postgres', fields({ port: 'abc' }), TLS_MODE.AUTO, false, false)).toBe('postgres://u:p%40ss@h/db');
    expect(buildDisplayUrl('postgres', fields({ port: '70000' }), TLS_MODE.AUTO, false, false)).toBe('postgres://u:p%40ss@h/db');
  });

  it('usa localhost sem host, colchetes em IPv6 e omite senha vazia', () => {
    expect(buildDisplayUrl('postgres', fields({ host: '', port: '', password: '' }), TLS_MODE.AUTO, false, false)).toBe('postgres://u@localhost/db');
    expect(buildDisplayUrl('postgres', fields({ host: '::1', port: '5432', database: '', password: '' }), TLS_MODE.AUTO, false, false)).toBe(
      'postgres://u@[::1]:5432'
    );
  });

  it('escreve o TLS na query igual a tls_query do núcleo', () => {
    const cases: Array<[ParsedConnectionUrl['dialect'], 'verify' | 'require' | 'disable', string]> = [
      ['postgres', 'verify', '?sslmode=verify-full'],
      ['postgres', 'require', '?sslmode=require'],
      ['postgres', 'disable', '?sslmode=disable'],
      ['mysql', 'verify', '?ssl-mode=VERIFY_IDENTITY'],
      ['mysql', 'require', '?ssl-mode=REQUIRED'],
      ['mysql', 'disable', '?ssl-mode=DISABLED'],
      ['sqlserver', 'verify', '?encrypt=true'],
      ['sqlserver', 'require', '?encrypt=true&trustServerCertificate=true'],
      ['sqlserver', 'disable', '?encrypt=false'],
    ];
    for (const [engine, tls, suffix] of cases) {
      expect(buildDisplayUrl(engine, fields(), tls, false, false).endsWith(suffix), `${engine} ${tls}`).toBe(true);
    }
    expect(buildDisplayUrl('postgres', fields(), TLS_MODE.AUTO, true, false)).not.toContain('?');
    expect(buildDisplayUrl('sqlserver', fields(), TLS_MODE.AUTO, true, false).endsWith('?trustServerCertificate=true')).toBe(true);
    expect(buildDisplayUrl('sqlserver', fields(), TLS_MODE.AUTO, false, false)).not.toContain('?');
  });

  it('SQLite mostra só o caminho', () => {
    expect(buildDisplayUrl('sqlite', fields({ path: ' /data/app.db ' }), TLS_MODE.REQUIRE, true, false)).toBe('/data/app.db');
    expect(buildDisplayUrl('sqlite', fields({ path: '' }), TLS_MODE.AUTO, false, false)).toBe('');
  });

  it('mascara a senha com *** só quando ela existe', () => {
    expect(buildDisplayUrl('postgres', fields(), TLS_MODE.AUTO, false, true)).toBe('postgres://u:***@h:5433/db');
    expect(buildDisplayUrl('postgres', fields({ password: '' }), TLS_MODE.AUTO, false, true)).toBe('postgres://u@h:5433/db');
  });

  it('formulário em branco gera string vazia (o campo mostra o placeholder)', () => {
    expect(buildDisplayUrl('postgres', fields({ host: '', port: '', database: '', user: '', password: '' }), TLS_MODE.AUTO, false, false)).toBe('');
  });
});

describe('sincronia string <-> campos', () => {
  it('editar um campo reescreve a string mascarada, com origem campo', () => {
    const m = filled();
    expect(m.origin).toBe(EDIT_ORIGIN.FIELD);
    expect(m.urlText).toBe('postgres://u:***@h:5433/db');
    m.setField('database', 'outro');
    expect(m.urlText).toBe('postgres://u:***@h:5433/outro');
  });

  it('colar uma string completa preenche campos, banco e TLS', () => {
    const m = new ConnectDbModel('postgres', fakeParser);
    m.setUrl('postgres://u:p%40ss@h:5433/db?sslmode=require');
    expect(m.origin).toBe(EDIT_ORIGIN.STRING);
    expect(m.urlError).toBeNull();
    expect(m.engine).toBe('postgres');
    expect(m.fields).toMatchObject({ host: 'h', port: '5433', database: 'db', user: 'u', password: 'p@ss' });
    expect(m.tls).toBe(TLS_MODE.REQUIRE);
  });

  it('o esquema troca o banco e mantém o que não pertence à string', () => {
    const m = filled();
    m.fields.schemas = 'public, sales';
    m.setUrl('mysql://root@db:3306/shop');
    expect(m.engine).toBe('mysql');
    expect(m.fields).toMatchObject({ host: 'db', port: '3306', user: 'root', database: 'shop', schemas: 'public, sales' });
  });

  it('um caminho de arquivo escolhe SQLite sem apagar os campos de rede', () => {
    const m = filled();
    m.setUrl('/data/app.db');
    expect(m.engine).toBe('sqlite');
    expect(m.fields.path).toBe('/data/app.db');
    expect(m.fields.host).toBe('h');
  });

  it('round-trip: campos -> string -> campos mantém todos os valores', () => {
    const secrets = ['p@ss', 'a/b:c%d', 'x y&z=1', "q'*()!", 'ç→日本'];
    for (const password of secrets) {
      const source = filled();
      source.setField('password', password);
      source.revealPassword();
      const full = source.urlText;
      const back = new ConnectDbModel('postgres', fakeParser);
      back.setUrl(full);
      expect(back.urlError, password).toBeNull();
      expect(back.fields).toMatchObject({ host: 'h', port: '5433', database: 'db', user: 'u', password });
    }
  });

  it('string inválida mostra erro, mantém os últimos campos válidos e some quando volta a valer', () => {
    const m = filled();
    m.setUrl('ftp://x');
    expect(m.urlError).toBe(CONNECT_URL_ERROR.UNKNOWN_SCHEME);
    expect(m.fields).toMatchObject({ host: 'h', user: 'u', password: 'p@ss' });
    expect(m.urlText).toBe('ftp://x');
    m.setUrl('postgres://u:p@h2:5432/db');
    expect(m.urlError).toBeNull();
    expect(m.fields.host).toBe('h2');
  });

  it('um código desconhecido do parser vira malformed e um objeto estranho não quebra', () => {
    const m = new ConnectDbModel('postgres', () => {
      throw new Error('boom');
    });
    m.setUrl('x');
    expect(m.urlError).toBe(CONNECT_URL_ERROR.MALFORMED);
  });

  it('string vazia não é erro nem apaga os campos', () => {
    const m = filled();
    m.setUrl('  ');
    expect(m.urlError).toBeNull();
    expect(m.fields.host).toBe('h');
  });

  it('editar um campo limpa o erro da string e reescreve o texto', () => {
    const m = filled();
    m.setUrl('ftp://x');
    m.setField('host', 'novo');
    expect(m.urlError).toBeNull();
    expect(m.urlText).toBe('postgres://u:***@novo:5433/db');
  });

  it('sem parser a string só é exibida e não derruba nada', () => {
    const m = new ConnectDbModel('postgres');
    expect(m.canParse).toBe(false);
    m.setUrl('mysql://a@b');
    expect(m.engine).toBe('postgres');
  });

  it('TLS e confiar no certificado reescrevem a string', () => {
    const m = filled();
    m.setTls(TLS_MODE.REQUIRE);
    expect(m.urlText.endsWith('?sslmode=require')).toBe(true);
    const ms = new ConnectDbModel('sqlserver', fakeParser);
    ms.setField('user', 'sa');
    ms.setField('host', 'h');
    ms.setTrustCert(true);
    expect(ms.urlText.endsWith('?trustServerCertificate=true')).toBe(true);
  });

  it('formulário em branco deixa a string vazia', () => {
    const m = new ConnectDbModel('postgres', fakeParser);
    expect(m.urlText).toBe('');
    expect(m.urlPlaceholder).toContain('postgres://');
    m.setEngine('sqlite');
    expect(m.urlPlaceholder).not.toContain('://');
  });

  it('trocar de banco preserva os dados e atualiza o esquema da string', () => {
    const m = filled();
    m.setEngine('mysql');
    expect(m.fields).toMatchObject({ host: 'h', port: '5433', database: 'db', user: 'u', password: 'p@ss' });
    expect(m.urlText).toBe('mysql://u:***@h:5433/db');
  });

  it('commitUrl reescreve a string digitada sem expor a senha', () => {
    const m = new ConnectDbModel('postgres', fakeParser);
    m.setUrl('postgres://u:segredo@h/db');
    expect(m.urlText).toContain('segredo');
    m.commitUrl();
    expect(m.urlText).toBe('postgres://u:***@h/db');
    expect(m.fields.password).toBe('segredo');
    expect(m.origin).toBe(EDIT_ORIGIN.STRING);
  });

  it('commitUrl não mexe numa string inválida nem com a senha revelada', () => {
    const m = new ConnectDbModel('postgres', fakeParser);
    m.setUrl('ftp://x');
    m.commitUrl();
    expect(m.urlText).toBe('ftp://x');
    m.setUrl('postgres://u:segredo@h/db');
    m.revealPassword();
    m.commitUrl();
    expect(m.urlText).toBe('postgres://u:segredo@h/db');
  });
});

describe('marcador *** da senha', () => {
  it('editar outra parte da string mascarada preserva a senha do campo', () => {
    const m = filled();
    m.setUrl(m.urlText.replace('@h:', '@novo:'));
    expect(m.fields.host).toBe('novo');
    expect(m.fields.password).toBe('p@ss');
    expect(m.urlError).toBeNull();
  });

  it('digitar uma senha real no lugar do marcador copia para o campo Senha', () => {
    const m = filled();
    m.setUrl(m.urlText.replace(PASSWORD_MASK, 'nova%2Fsenha'));
    expect(m.fields.password).toBe('nova/senha');
  });

  it('remover a senha da string limpa o campo Senha', () => {
    const m = filled();
    m.setUrl('postgres://u@h:5433/db');
    expect(m.fields.password).toBe('');
  });
});

describe('revelação da senha', () => {
  it('começa oculta: a string mostra user:***@host', () => {
    const m = filled();
    expect(m.passwordRevealed).toBe(false);
    expect(m.urlText).toContain('u:***@h');
    expect(m.urlText).not.toContain('p%40ss');
  });

  it('alternar mostra o valor real na string e oculta de novo', () => {
    const m = filled();
    m.togglePassword();
    expect(m.passwordRevealed).toBe(true);
    expect(m.urlText).toBe('postgres://u:p%40ss@h:5433/db');
    m.togglePassword();
    expect(m.passwordRevealed).toBe(false);
    expect(m.urlText).toBe('postgres://u:***@h:5433/db');
  });

  it('volta a oculto ao fechar, ao concluir a conexão e ao trocar de banco', () => {
    const m = filled();
    m.revealPassword();
    m.close();
    expect(m.passwordRevealed).toBe(false);
    m.revealPassword();
    m.finish({ dbml: 'Table a {\n  id int\n}\n', warnings: [], tables: 1 });
    expect(m.passwordRevealed).toBe(false);
    m.revealPassword();
    m.setEngine('mysql');
    expect(m.passwordRevealed).toBe(false);
    expect(m.urlText).toContain(':***@');
  });

  it('alternar não sobrescreve uma string inválida em edição', () => {
    const m = filled();
    m.setUrl('ftp://x');
    m.togglePassword();
    expect(m.urlText).toBe('ftp://x');
  });

  it('copiar entrega a string mascarada, ou completa só com a senha revelada', () => {
    const m = filled();
    expect(m.urlForCopy()).toBe('postgres://u:***@h:5433/db');
    m.revealPassword();
    expect(m.urlForCopy()).toBe('postgres://u:p%40ss@h:5433/db');
  });

  it('a cópia usa os campos mesmo quando a string digitada está inválida', () => {
    const m = filled();
    m.setUrl('ftp://x');
    expect(m.urlForCopy()).toBe('postgres://u:***@h:5433/db');
  });

  it('a revelação não entra no pedido de introspecção nem no estado persistível', () => {
    const m = filled();
    m.revealPassword();
    expect(Object.keys(m.buildRequest())).not.toContain('passwordRevealed');
    expect(m.buildRequest()).toMatchObject({ host: 'h', port: 5433, user: 'u', password: 'p@ss', dialect: 'postgres' });
  });
});

describe('mensagens de erro da string', () => {
  it('cada código tem texto curto em pt-BR e en', () => {
    for (const code of Object.values(CONNECT_URL_ERROR)) {
      setLanguage('pt-BR');
      const pt = urlErrorMessage(code);
      setLanguage('en');
      const en = urlErrorMessage(code);
      expect(pt.length).toBeGreaterThan(0);
      expect(en.length).toBeGreaterThan(0);
      expect(en).not.toBe(pt);
    }
    setLanguage('pt-BR');
  });
});

describe('navegação do dropdown', () => {
  const options = [{ disabled: false }, { disabled: false }, { disabled: true }, { disabled: false }];

  it('anda pelas opções habilitadas e pula as desabilitadas', () => {
    expect(nextEnabledOption(options, 1, 1)).toBe(3);
    expect(nextEnabledOption(options, 3, -1)).toBe(1);
  });

  it('não passa dos extremos', () => {
    expect(nextEnabledOption(options, 3, 1)).toBe(3);
    expect(nextEnabledOption(options, 0, -1)).toBe(0);
  });

  it('Home e End vão à primeira e à última habilitada', () => {
    expect(nextEnabledOption(options, 1, 'first')).toBe(0);
    expect(nextEnabledOption([{ disabled: false }, { disabled: true }], 0, 'last')).toBe(0);
    expect(nextEnabledOption(options, 0, 'last')).toBe(3);
  });
});
