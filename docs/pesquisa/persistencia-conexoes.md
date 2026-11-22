# Persistência de conexões de banco (host, usuário, senha) em repouso: como as ferramentas fazem

Data: 2026-11-22. Objetivo: subsidiar o desenho de "conexões salvas" do DB Modeling (Desktop GPUI, CLI Web `dbml run`, extensão VS Code, Docker headless). Nada foi implementado.

## Aviso de método e lacunas (ler primeiro)

- As fontes foram obtidas por **busca web com resumo** (páginas de documentação oficial, wiki do DBeaver, docs do pgAdmin, docs do Electron, docs.rs, bugs do MySQL, gists públicos). **Não li o código-fonte linha a linha** (o fetch direto de páginas foi bloqueado no ambiente). Portanto, onde a tabela diz "código" é porque a fonte secundária cita o arquivo; o que não foi visto em fonte aparece como **não confirmado**.
- Lacunas explícitas: TablePlus, Beekeeper Studio (armazenamento local), DbGate (edição comunitária), Azure Data Studio, SQLTools / vscode-postgres / Database Client, Postman, Sequel Ace (detalhe do Keychain): **sem fonte confirmando o mecanismo**. Não afirmo nada de memória sobre eles.
- Valores numéricos (iterações, memória) mudam; conferir a página oficial antes de fixar no código.

## 1. Tabela comparativa

| Ferramenta | Onde / como armazena | Cripto | Origem da chave | Fraquezas conhecidas | Fonte |
|---|---|---|---|---|---|
| DBeaver (Community) | `credentials-config.json` no workspace do projeto; usuário e senha num arquivo criptografado | Docs antigas (2021 a 2023) dizem AES; docs de 23.3 em diante/wiki atual dizem "DES key" (a própria doc é inconsistente). Gists de terceiros mostram AES-CBC com IV nos 16 primeiros bytes | **Chave fixa no código-fonte**, pública | A doc oficial admite: a chave é pública nos fontes e terceiros podem decifrar o arquivo. Existem decifradores prontos (`dbeaver-creds`, gists, módulo Metasploit) | [Project security (24.2)](https://dbeaver.com/docs/dbeaver/24.2/Project-security/), [wiki](https://github.com/dbeaver/dbeaver/wiki/Project-security), [Tatsh/dbeaver-creds](https://github.com/Tatsh/dbeaver-creds), [gist](https://gist.github.com/lgh06/97c0ba30b2e7326595ae696b3ea16113) |
| DBeaver (Enterprise/Team) | Mesmo arquivo, mas protegido por **senha mestra** (Preferences > Security) ou senha por projeto; opção "Windows Integration" | Não confirmado (a doc consultada não nomeia o algoritmo) | Senha mestra do usuário | Doc consultada é da era 21.x; detalhe atual não confirmado. Recurso só nas edições pagas | [Project security (21.3)](https://dbeaver.com/docs/dbeaver/21.3/Project-security/), [Managing Master Password (25.1)](https://dbeaver.com/docs/dbeaver/25.1/Managing-Master-Password/) |
| pgAdmin 4 (desktop) | Senhas salvas no SQLite do pgAdmin | Desde a 7.2 usa o **armazenamento de senhas do SO** por padrão; se indisponível, cai na senha mestra | Keychain do SO ou senha mestra | Com senha mestra desligada (`MASTER_PASSWORD_REQUIRED=False`) as senhas salvas são **removidas** (não ficam em claro). Em versão antiga/sem mestra, a chave ficava no mesmo SQLite que as senhas (EDB). Resetar a mestra apaga todas as senhas salvas | [Master Password 7.8](https://www.pgadmin.org/docs/pgadmin4/7.8/master_password.html), [9.8](https://www.pgadmin.org/docs/pgadmin4/9.8/master_password.html), [release 7.2](https://www.postgresql.org/about/news/pgadmin-4-v72-released-2647/), [blog EDB](https://www.enterprisedb.com/blog/use-master-password-pgadmin-4) |
| DataGrip / JetBrains | "Password Safe" com três modos: **keychain nativo do SO**, **KeePass** (`c.kdbx`) ou não salvar (esquece após reiniciar) | KeePass (KDBX) no modo arquivo; keychain no modo nativo. Algoritmo exato do KDBX nesse uso: não confirmado | KeePass: senha mestra definida pelo usuário. Sem senha mestra definida: relato de usuário no fórum diz que o arquivo de chave é cifrado com **chave fixa** no código (`KeePassCredentialStore.kt`): **não confirmado, só relato** | Padrão sem senha mestra seria obscuridade (relato). Keychain nativo só para macOS/Linux em algumas versões da doc | [Passwords (IDEA)](https://www.jetbrains.com/help/idea/reference-ide-settings-password-safe.html.md), [fórum JetBrains](https://intellij-support.jetbrains.com/hc/en-us/community/posts/115000071690/comments/115000082110) |
| HeidiSQL | Registro do Windows (ou `portable_settings.txt` na versão portátil, formato exato **não confirmado**) | **Nenhuma real**: ofuscação reversível (deslocamento de um dígito por caractere + hex, segundo gists) | Nenhuma | Trivial de reverter; há módulo Rapid7 que coleta | [Rapid7 post/windows/gather/credentials/heidisql](https://www.rapid7.com/db/modules/post/windows/gather/credentials/heidisql/), [gist](https://gist.github.com/jpruiz114/afe5c7da9ae641eac9cc) |
| MySQL Workbench | Linux: libsecret/Secret Service (gnome-keyring). macOS: Keychain. Windows: a doc fala em arquivo cifrado no diretório de dados do Workbench (detalhe **não confirmado**) | Delegada ao keyring do SO | Login do usuário no SO | No Linux, sem `gnome-keyring-daemon` (KDE, headless, WSL) a senha não persiste: erro `org.freedesktop.secrets ... not provided by any service file`; vários bugs abertos por anos | [Manual: vault](https://dev.mysql.com/doc/workbench/en/wb-mysql-connections-vault.html), [Bug 91836](https://bugs.mysql.com/bug.php?id=91836), [Bug 107269](https://bugs.mysql.com/bug.php?id=107269), [Bug 62919](https://bugs.mysql.com/bug.php?id=62919) |
| Sequel Ace | `Favorites.plist` (sem senha) + **macOS Keychain** (senha), segundo a doc | Keychain | Login do macOS | Migrar do Sequel Pro não leva as senhas. Detalhe do item (ACL do app) não confirmado | [Migrating from Sequel Pro](https://sequel-ace.com/get-started/migrating-from-sequel-pro.html) |
| Beekeeper Studio | Dados locais em `~/.config/beekeeper-studio/` (Linux), `%APPDATA%\beekeeper-studio\`, `~/Library/Application Support/beekeeper-studio/`. Campos sensíveis são "criptografados pela aplicação" antes de gravar; nuvem usa AES-GCM 256 em nível de campo | Local: **não confirmado**. Nuvem: AES-GCM 256 (doc oficial) | Local: **não confirmado** | Sem fonte do mecanismo local | [Trust](https://beekeeperstudio.io/trust/), [Security docs](https://docs.beekeeperstudio.io/user_guide/security/) |
| DbGate | Edição comunitária: não confirmado. Edição Team: senha cifrada com **chave aleatória gerada no primeiro start** (não dá para copiar o valor cifrado para outra instalação) | Não confirmado | Chave aleatória local (Team) | Sem fonte para o desktop comunitário | [DbGate 6.3.3](https://dbgate.io/news/2025-04-09-6-3-3-admin-ui) |
| TablePlus | Sem fonte encontrada | não confirmado | não confirmado | Lacuna total | (nenhuma) |
| Azure Data Studio | Sem fonte do mecanismo. Produto **aposentado em 28/02/2026** (Microsoft) | não confirmado | não confirmado | Irrelevante como referência futura | [Azure connectivity](https://learn.microsoft.com/sql/azure-data-studio/azure-connectivity) |
| Extensões VS Code de banco (SQLTools, vscode-postgres, Database Client) | SQLTools: discussão no GitHub diz que alguns drivers usam recurso do SQLTools para guardar senha no keychain do usuário; chaves exatas de `settings.json` e comportamento padrão **não confirmados**. Demais: não confirmado | via keychain (SQLTools, segundo a discussão) | SO | Sem verificação do padrão; risco de senha em `settings.json` em claro (não confirmado) | [SQLTools discussion #1327](https://github.com/mtxr/vscode-sqltools/discussions/1327) |
| VS Code `SecretStorage` (API) | `context.secrets`: por extensão, global (independe do workspace), **não sincroniza** entre máquinas | "Encrypted" (doc). Implementação: Electron `safeStorage` -> OSCrypt do Chromium (Keychain no macOS, DPAPI no Windows, libsecret/kwallet no Linux), segundo análise da Cycode | Chave guardada no keyring do SO | Qualquer extensão instalada pode usar a API; análise da Cycode mostra roubo de segredos por extensão maliciosa. Em Linux sem keyring cai no backend fraco do Electron (ver abaixo). Detalhe do código do VS Code: não confirmado diretamente | [Cycode: VS Code secrets](https://cycode.com/blog/exposing-vscode-secrets/), [Control Plane: parte 2](https://control-plane.io/posts/abusing-vscode-from-malicious-extensions-to-stolen-credentials-part-2/), [Electron safeStorage](https://electronjs.org/docs/latest/api/safe-storage) |
| Electron `safeStorage` | Cifra/decifra strings; quem guarda o resultado é o app | macOS Keychain, Windows DPAPI, Linux kwallet/kwallet5/kwallet6/gnome-libsecret | SO | **No Linux sem secret store usa o backend `basic_text`: senha fixa em texto claro no código, e `isEncryptionAvailable()` ainda retorna verdadeiro**; é preciso checar `getSelectedStorageBackend()`. API síncrona marcada para remoção no Electron 46. macOS não assinado causa prompts repetidos | [Electron safeStorage](https://electronjs.org/docs/latest/api/safe-storage), [Clerk (aviso basic_text)](https://clerk.com/docs/reference/electron/storage) |
| Insomnia | Variáveis de ambiente "secret" cifradas localmente com uma "vault key"; PR 2025 migrou para `safeStorage` | AES (via vault key); detalhe não confirmado | `safeStorage` / SO | Doc de segurança diz que dados locais **não** são cifrados em disco fora dos secrets; variáveis normais ficam em claro | [Secret environment variables](https://docs.insomnia.rest/insomnia/secret-environment-variables), [PR #8233](https://github.com/Kong/insomnia/pull/8233), [Data security](https://developer.konghq.com/insomnia/data-security.md) |
| Postman | Sem fonte encontrada | não confirmado | não confirmado | Lacuna | (nenhuma) |
| Tauri Stronghold | Snapshot cifrado (IOTA Stronghold) | Chave de 32 bytes vinda de uma função de hash da senha; padrão do plugin é **Argon2** | Senha fornecida pelo app/usuário | Repositório espelho somente leitura (último push out/2024; desenvolvimento no `plugins-workspace`); sem aviso de depreciação encontrado | [Tauri v2 Stronghold](https://v2.tauri.app/plugin/stronghold), [docs.rs](https://docs.rs/tauri-plugin-stronghold) |
| Bitwarden (referência) | Cofre cifrado no cliente | PBKDF2-SHA256 (600k/700k, as páginas divergem) ou Argon2id (padrão 32 MiB, 6 iterações, 4 threads); dados em AES-CBC 256 + HMAC | Senha mestra (+ KDF) | Trocar o KDF só recifra a chave simétrica protegida. `BW_SESSION` do CLI: **não confirmado** | [KDF algorithms](https://bitwarden.com/help/kdf-algorithms/), [White paper](https://bitwarden.com/help/bitwarden-security-white-paper/) |
| 1Password CLI (referência) | Não pesquisado | não confirmado | não confirmado | Lacuna | (nenhuma) |

## 2. Resumo das abordagens

### a) Keychain do SO
- Como: macOS Keychain, Windows (Credential Manager/DPAPI), Linux Secret Service (libsecret; gnome-keyring ou KWallet).
- Prós: sem senha extra para o usuário; a chave nunca vai para o nosso arquivo; desbloqueia com o login; é o padrão que MySQL Workbench, Sequel Ace, pgAdmin 7.2+, JetBrains (modo nativo) e `SecretStorage` adotam.
- Contras: Linux depende de daemon D-Bus + keyring desbloqueado, o que **falha em headless, Docker, WSL, SSH, KDE sem serviço** (bugs recorrentes do MySQL Workbench). Flatpak/Snap precisam do Secret **portal** (`org.freedesktop.portal.Secret`, API diferente do Secret Service, exige backend; KWallet não implementa na época dos relatos). Ainda não protege de processo do mesmo usuário (no macOS a ACL por app ajuda só se o binário for assinado).
- Crate Rust `keyring` (3.6.3): Linux oferece Secret Service, `keyutils` do kernel (em memória, **perde tudo no reboot**) e um modo híbrido (keyutils como cache + Secret Service para persistir, exige processo "com cabeça" para desbloquear após reboot). Documentação recomenda keyutils para headless. Sem feature de plataforma, a crate cai num **store mock** (conferir a versão). Comportamento dentro de contêiner Docker: **não confirmado**. Fonte: [keyring 3.6.3 README](https://docs.rs/crate/keyring/3.6.3/source/README.md), [keyutils](https://docs.rs/crate/keyring/3.1.0/source/src/keyutils.rs).

### b) Senha mestra (KDF + AEAD)
- Como: derivar chave com Argon2id (OWASP: mínimo 19 MiB, t=2, p=1; scrypt N=2^17 r=8 p=1 como alternativa; PBKDF2-HMAC-SHA256 ~600k só se precisar de FIPS) e cifrar com AES-GCM ou XChaCha20-Poly1305. Referências: Bitwarden, KeePass/KDBX (JetBrains), pgAdmin (mestra), DBeaver Enterprise, Tauri Stronghold. Fonte OWASP: [Password Storage Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Password_Storage_Cheat_Sheet.html) (valores conferidos por resumo; confirmar na página).
- Prós: funciona em qualquer SO, Docker e headless; protege arquivo roubado/backup/dotfiles mesmo se o atacante tiver a máquina desligada; portátil entre máquinas (se o usuário quiser).
- Contras: fricção (digitar senha toda sessão, ou manter chave em memória); senha esquecida = tudo perdido (pgAdmin apaga as salvas); UX para CLI web (onde digitar? prompt no terminal ou tela de desbloqueio) e para VS Code. Segurança vale o que vale a senha. Após desbloquear, malware do mesmo usuário lê a memória.

### c) Arquivo de chave (0600) ou `age`
- Como: chave aleatória em arquivo separado do cofre com permissão 0600 (ou `age` com identidade em arquivo / passphrase).
- Prós: sem daemon, funciona em Docker (montando volume), sem fricção.
- Contras: protege só quando o arquivo de dados vaza **sem** o arquivo de chave (ex.: dotfiles/nuvem que copiam só o `.dbml.layout.json`). Se backup copiar a pasta toda, a chave vai junto. Quem lê como o mesmo usuário lê ambos. No Windows, 0600 não existe (ACL). `age` não é fonte pesquisada neste relatório: **não confirmado** por fonte.

### d) Chave embutida no binário
- É o que o DBeaver Community faz (chave pública nos fontes; a própria doc admite). HeidiSQL nem isso (ofuscação). É segurança por obscuridade: quem tem o binário ou o código-fonte (nosso projeto é inspecionável) decifra em segundos; ferramentas públicas já fazem isso para o DBeaver. Pode servir só como "não deixar em texto puro" contra bisbilhoteiro casual, mas **não atende ao requisito** do usuário. Não recomendado.

### e) Não salvar a senha
- Salvar host/usuário/porta/banco; pedir a senha a cada sessão (modo "esquecer após reiniciar" do JetBrains, "ask on connect"). Prós: zero segredo em repouso, zero dependência de SO, funciona em qualquer lugar. Contras: fricção; precisa de prompt na CLI web/VS Code. Deve existir como **modo padrão seguro/fallback**.

## 3. Modelo de ameaça

| Ameaça | Keychain SO | Senha mestra | Arquivo de chave 0600 | Chave embutida | Não salvar |
|---|---|---|---|---|---|
| Arquivo roubado/backup/dotfiles em git ou nuvem (sem o resto da conta) | Protege (segredo fica fora do arquivo) | Protege (depende da força da senha) | Protege só se a chave não for junto | **Não protege** (chave pública) | Nada a roubar |
| Disco inteiro copiado com a conta desligada | Protege se keychain bloqueado por senha de login; fraco se login automático | Protege | Não protege (chave no mesmo disco) | Não protege | Protege |
| Malware rodando como o mesmo usuário, sessão aberta | **Não protege** (pode pedir ao keyring; macOS pede confirmação por app, outros não) | **Não protege** depois do desbloqueio (lê da memória/espera a digitação) | Não protege | Não protege | Parcial (só captura quando o usuário digita) |
| Outro usuário da mesma máquina | Protege | Protege | Protege se 0600 | Não protege | Protege |

Limite honesto: criptografia em repouso protege contra **vazamento de arquivo**, não contra execução de código como o mesmo usuário. Extensões VS Code maliciosas podem ler `SecretStorage` de outras extensões (Cycode, Control Plane), e `safeStorage` em Linux sem keyring vira obscuridade.

## 4. O que muda para nós

| Modalidade | Situação | Consequência |
|---|---|---|
| Desktop nativo (GPUI) | Controle total do processo e do ciclo de vida. Linux: Secret Service costuma existir em GNOME/KDE com sessão; Flatpak exigiria portal | Keychain via `keyring` é natural. Precisa de **detecção de indisponibilidade** com fallback visível ao usuário (nunca falhar em silêncio, nunca cair em claro) |
| CLI Web local (`dbml run`, Axum) | Mesmo binário roda em terminal, serviço, WSL e Docker; o navegador não tem acesso ao keychain, só o processo servidor | O servidor é quem cifra/decifra; a página web nunca recebe a senha de volta (só "tem senha salva: sim/não") |
| Extensão VS Code | `SecretStorage` já resolve por extensão, sem dependência nossa; não sincroniza (bom) | Usar `context.secrets` direto; documentar que outras extensões têm a mesma superfície de ataque. Manter o núcleo Wasm sem acesso a segredos |
| Docker headless | Sem D-Bus/keyring, sem sessão de login; `keyutils` some no reboot/recriação do contêiner; chave em arquivo no volume protege pouco | Aqui só funcionam: (1) **não salvar senha**, (2) senha mestra, (3) chave fornecida por variável de ambiente/secret do Docker (`*_FILE`) sem persistir a chave junto. Variável de ambiente é um segredo exposto ao processo, mas é o padrão do ecossistema Docker |

Reforço do princípio do projeto: DBML do usuário continua DBML válido e sem segredos; o sidecar `<schema>.dbml.layout.json` **não** deve receber senha. Conexões salvas devem ir para um arquivo próprio no diretório de configuração do usuário (não no diretório do esquema, que vai para git), com **host/usuário/porta/banco em claro e só a senha em cofre** (como Sequel Ace e Tabularis fazem; só a senha precisa de proteção) ou o arquivo inteiro cifrado, se o usuário considerar host/usuário sensíveis. As constantes (nome de serviço do keychain, nomes de arquivo, parâmetros do KDF) entram em `constants.rs`/`constants.ts`, conforme o projeto.

## 5. Recomendação

Estratégia em camadas, com **falha explícita, nunca em claro**:

1. **Padrão no Desktop e na CLI com sessão de login:** senha no **keychain do SO** (crate `keyring` com o backend nativo de cada SO). Metadados sem senha num arquivo de configuração do usuário (0600).
2. **Fallback quando não há keychain (Docker, headless, WSL, Flatpak sem portal):** oferecer **senha mestra** (Argon2id nos parâmetros OWASP ou mais fortes + AEAD XChaCha20-Poly1305 ou AES-256-GCM, sal e nonce aleatórios por registro/cofre, versão do formato no cabeçalho) **ou** o modo **não salvar senha** (pedir a cada sessão). Avisar claramente qual modo está ativo.
3. **VS Code:** `SecretStorage`.
4. **Rejeitar:** chave embutida no binário, ofuscação, e qualquer fallback silencioso para texto claro (o problema do `basic_text` do Electron e do mock do `keyring`).

Justificativa: atende ao requisito (só quem usa a conta acessa) no caso comum sem fricção; o caso Docker/headless, onde o keychain não existe, ganha opções honestas em vez de falsa segurança; segue o que MySQL Workbench, Sequel Ace, pgAdmin 7.2+ e VS Code já fazem.

### Decisões em aberto (para o usuário)
1. Fallback sem keychain: senha mestra, "não salvar", ou ambos? Se mestra, ela é pedida no start do servidor (terminal) ou numa tela de desbloqueio na web?
2. Cifrar só a senha ou o arquivo de conexões inteiro (host/usuário também)?
3. Onde fica o arquivo de conexões (diretório de config do usuário, não o do esquema)? E o Docker: volume + `*_FILE`?
4. Suporte a Flatpak/Snap no Desktop (exige portal Secret) agora ou depois?
5. Compartilhar conexões entre Desktop, CLI e VS Code (mesmo cofre) ou cofres separados? `SecretStorage` não é acessível fora do VS Code, então o cofre da extensão será necessariamente separado.
6. Migração/rotação: versão do formato, troca de senha mestra (recifrar), esqueci a senha (apagar e pedir de novo, como pgAdmin).
7. Parâmetros do KDF e a escolha AES-GCM vs XChaCha20-Poly1305: confirmar na página atual do OWASP e nas crates RustCrypto antes de fixar versão (fora do escopo desta pesquisa).

## 6. Fontes

- DBeaver, Project security: https://dbeaver.com/docs/dbeaver/24.2/Project-security/ , https://dbeaver.com/docs/dbeaver/23.3/Project-security , https://dbeaver.com/docs/dbeaver/21.3/Project-security/ , https://github.com/dbeaver/dbeaver/wiki/Project-security
- DBeaver, Managing Master Password: https://dbeaver.com/docs/dbeaver/25.1/Managing-Master-Password/
- Decifradores do DBeaver (terceiros): https://github.com/Tatsh/dbeaver-creds , https://gist.github.com/lgh06/97c0ba30b2e7326595ae696b3ea16113 , https://docs.metasploit.com/api/Rex/Parser/Dbeaver.html
- pgAdmin: https://www.pgadmin.org/docs/pgadmin4/9.8/master_password.html , https://www.pgadmin.org/docs/pgadmin4/7.8/master_password.html , https://www.postgresql.org/about/news/pgadmin-4-v72-released-2647/ , https://www.enterprisedb.com/blog/use-master-password-pgadmin-4
- JetBrains: https://www.jetbrains.com/help/idea/reference-ide-settings-password-safe.html.md , https://intellij-support.jetbrains.com/hc/en-us/community/posts/115000071690/comments/115000082110
- HeidiSQL: https://www.rapid7.com/db/modules/post/windows/gather/credentials/heidisql/ , https://gist.github.com/jpruiz114/afe5c7da9ae641eac9cc
- MySQL Workbench: https://dev.mysql.com/doc/workbench/en/wb-mysql-connections-vault.html , https://bugs.mysql.com/bug.php?id=91836 , https://bugs.mysql.com/bug.php?id=107269 , https://bugs.mysql.com/bug.php?id=62919
- Sequel Ace: https://sequel-ace.com/get-started/migrating-from-sequel-pro.html
- Beekeeper Studio: https://beekeeperstudio.io/trust/ , https://docs.beekeeperstudio.io/user_guide/security/
- DbGate: https://dbgate.io/news/2025-04-09-6-3-3-admin-ui
- Azure Data Studio (aposentadoria): https://learn.microsoft.com/sql/azure-data-studio/azure-connectivity
- SQLTools: https://github.com/mtxr/vscode-sqltools/discussions/1327
- VS Code SecretStorage: https://cycode.com/blog/exposing-vscode-secrets/ , https://control-plane.io/posts/abusing-vscode-from-malicious-extensions-to-stolen-credentials-part-2/ (a página oficial https://code.visualstudio.com/api/references/vscode-api não foi lida diretamente)
- Electron safeStorage: https://electronjs.org/docs/latest/api/safe-storage , https://clerk.com/docs/reference/electron/storage
- Insomnia: https://docs.insomnia.rest/insomnia/secret-environment-variables , https://github.com/Kong/insomnia/pull/8233 , https://developer.konghq.com/insomnia/data-security.md
- Tauri Stronghold: https://v2.tauri.app/plugin/stronghold , https://docs.rs/tauri-plugin-stronghold
- Bitwarden: https://bitwarden.com/help/kdf-algorithms/ , https://bitwarden.com/help/bitwarden-security-white-paper/
- keyring-rs: https://docs.rs/crate/keyring/3.6.3/source/README.md , https://docs.rs/crate/keyring/3.1.0/source/src/keyutils.rs
- OWASP: https://cheatsheetseries.owasp.org/cheatsheets/Password_Storage_Cheat_Sheet.html
- Flatpak Secret portal: https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Secret.html , https://bugs.kde.org/show_bug.cgi?id=466197 , https://snapcraft.io/docs/how-to-guides/snap-development/use-the-secret-portal/
- Tabularis (apenas comparação de padrão, não é TablePlus): https://tabularis.dev/wiki/security-credentials
- Sem fonte (lacunas): TablePlus, Postman, 1Password CLI, `age`, Database Client e vscode-postgres.
