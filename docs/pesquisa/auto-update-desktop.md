# Pesquisa: como o auto-update de apps desktop funciona (Zed, VS Code e referências)

Data da pesquisa: 2026-11-23. Objetivo: embasar `.scratch/auto-update-desktop/spec.md` e
`openspec/adr/0010-auto-update-desktop.md` do DB Modeling.

Convenções de marcação:

- **CONFIRMADO**: lido diretamente no código-fonte ou na documentação oficial, com URL e trecho/linha.
- **NÃO CONFIRMADO**: dedução, conhecimento geral ou resultado de busca que não foi aberto na fonte primária.
- **Commit do Zed estudado:** `6501df15765d66d6c2a2b66b1eb22434ccef3f86` (último commit da `main` até
  2026-11-23T23:59:59Z, feito às 17:21:43Z; obtido com `gh api repos/zed-industries/zed/commits?until=...`).
  Os arquivos foram lidos na `main` durante o dia 2026-11-23; conferimos por amostragem neste commit que os números
  de linha do `auto_update.rs` batem (`POLL_INTERVAL` na 47, `start_polling` na 490, `install_release_linux` na
  1129, `install_release_macos` na 1197), mas as demais citações não foram reconferidas uma a uma. Os links de
  código do Zed em "Fontes" apontam para este commit.
- **Desenho avaliado:** as seções "Comparação com o DB Modeling" e "Riscos e recomendações" analisam o **desenho
  inicial** do ADR (canais em runtime, `pkexec`, atraso de 10 s e checagem a cada 6 h, `DBM_UPDATE`). O desenho final
  está em `openspec/adr/0010-auto-update-desktop.md` e em [docs/auto-update.md](../auto-update.md); as
  recomendações R1 a R8 viraram, em sua maior parte, requisitos do spec.

## Resumo

- **Zed** consulta um endpoint próprio (`/releases/{canal}/latest/asset?asset=zed&os=&arch=`) a cada 1 h
  (nightly: 15 min). O endpoint devolve `{version, url}`. Baixa o artefato para um diretório temporário e o instala
  no lugar. No macOS monta o `.dmg` e faz `rsync` para o `.app`. No Linux extrai o `tar.gz` e faz `rsync` para
  `~/.local` ou para o prefixo atual. No Windows roda o instalador Inno com `/verysilent /update=true` e usa um
  helper (`auto_update_helper.exe`) que troca os binários depois que o Zed sai. **O cliente do Zed não verifica
  hash nem assinatura do artefato** (nenhuma ocorrência no `auto_update.rs`). Confia só em TLS e no servidor.
- **VS Code** tem um `UpdateService` por plataforma sobre uma base comum. A URL é
  `{updateUrl}/api/update/{plataforma}/{qualidade}/{commit}`. No Windows o app baixa o `CodeSetup-*.exe`, confere o
  `sha256hash` quando o servidor o informa e roda o Inno em `/verysilent`, coordenado por mutexes nomeados e
  arquivos `.flag`. No macOS delega ao `autoUpdater` do Electron (Squirrel.Mac). No Linux não instala nada: abre a
  página de download. O modo é `update.mode` = `none | manual | start | default`.
- **Ferramentas de referência** (Tauri, Sparkle, WinSparkle) exigem assinatura Ed25519 sobre o artefato, com chave
  pública embutida no app. Todas avisam que perder a chave privada impede atualizar quem já instalou. Só o
  `self_update` e o `axoupdater` do cargo-dist cobrem o caso "binário Rust se atualizando", e nenhum cobre
  `.deb`/`.rpm` com elevação.
- **Comparação**: o desenho do DB Modeling (manifesto assinado + SHA-256 + recusa de downgrade) é mais forte que o
  do Zed e comparável ao do Tauri/Sparkle. Os pontos fracos estão na execução: condição de corrida entre verificar
  e instalar com privilégio (Linux/Windows por máquina), arquivo em uso no Windows, rotação de chave e replay/freeze
  de manifesto.

## Zed

Fonte principal: `crates/auto_update/src/auto_update.rs` e `crates/auto_update_ui/src/auto_update_ui.rs` em
https://github.com/zed-industries/zed (branch `main`, baixado em 2026-11-23).

### Verificação (checagem)

- **CONFIRMADO** - Intervalo: `POLL_INTERVAL = 60 * 60 s` (1 h) e `NIGHTLY_POLL_INTERVAL = 15 * 60 s`
  (`auto_update.rs` linhas 47-48). `start_polling` faz `poll(Automatic)` e dorme o intervalo (linhas 490-515).
- **CONFIRMADO** - Endpoint: `get_release_asset` monta o caminho
  `format!("/releases/{}/{}/asset", release_channel.dev_name(), version)` com `version = "latest"`. Os parâmetros de
  query vêm de `AssetQuery { asset, os, arch, metrics_id, system_id, is_staff }` (linhas 113-120 e 677-740). Usa
  `build_zed_cloud_url_with_query`. O host é **`cloud.zed.dev`**: **CONFIRMADO** em `script/install.sh` linha 86 do
  commit estudado (`https://cloud.zed.dev/releases/$channel/$ZED_VERSION/download?asset=zed&arch=$arch&os=linux&source=install.sh`).
  Essa linha usa a rota `/download`; para a rota `/asset` do updater só o caminho foi lido no código, não o host.
- **CONFIRMADO** - Payload de resposta: `ReleaseAsset { version: String, url: String }` (linhas 193-196). É JSON
  simples, sem hash nem assinatura.
- **CONFIRMADO** - Identificadores de ativo: o `asset` é `"zed"` e o nome do arquivo local é `zed.tar.gz` (Linux),
  `Zed.exe` (Windows) ou `.dmg` (macOS) (`target_path`, linhas 923-931).
- **CONFIRMADO** - Telemetria: `metrics_id`, `system_id` e `is_staff` só vão na query se
  `telemetry().metrics_enabled()` (linhas 686-695).
- **CONFIRMADO** - Comparação de versão (linhas 960-967): stable e preview zeram `pre` e `build` e exigem
  `fetched > installed`. O nightly compara o SHA do commit (linhas 878-886) em vez de semver.
- **CONFIRMADO** - Erros: checagem automática com falha vira `Idle` em silêncio (log `info`). Só erro por falta de
  dependência (`rsync`) vira `Errored`. A checagem manual mostra `Errored` (linhas 541-580).
- **CONFIRMADO** - Retoma após suspensão: `restart_after_wake` reinicia só fases de rede, nunca `Installing`
  (linhas 474-488).

### Canais

- **CONFIRMADO** - `ReleaseChannel` tem `Dev`, `Nightly`, `Preview` e `Stable`. `poll_for_updates()` devolve
  `!matches!(self, Dev)` (`crates/release_channel/src/lib.rs`, linha 201).
- **CONFIRMADO** - O preview "recebe atualizações cerca de uma semana antes do estável"
  (`docs/src/installation.md` linha 54). O canal é fixado no binário, não é uma opção de runtime. Escolher outro
  canal significa instalar outro app (`ZED_CHANNEL=preview` no `install.sh`, `zed@preview` no Homebrew).

### Download e integridade

- **CONFIRMADO** - `download_release` (linhas 1076-1127): `GET` na URL do `ReleaseAsset`, escrita em streaming num
  buffer de 8 KiB, progresso por `Content-Length` informado a cada percentual inteiro. Não há `Range`/retomada, hash
  nem assinatura. Busca por `sha256|signature|verify|checksum` no arquivo não retornou nada relevante (só
  `AppCommitSha`).
- **Consequência (inferência)**: a segurança do Zed depende de TLS e da conta do servidor. Um servidor ou CDN
  comprometido entrega código arbitrário. Não é referência de segurança para o nosso desenho.
- **CONFIRMADO** - Diretório de instalação: fora do Windows usa `tempfile::TempDir` com prefixo. No Windows usa um
  diretório ao lado do `Zed.exe` (linhas 372-410). Faxina de diretórios velhos com mais de 24 h no temp
  (`cleanup_stale_installer_dirs`, linhas 1253-1295).

### Instalação por SO

- **CONFIRMADO - macOS** (`install_release_macos`, linhas 1197-1250): `hdiutil attach -nobrowse <dmg> -mountroot
  <tmp>`, depois `rsync -av --delete --exclude "Icon?" <montado>/ <app em execução>`, e por fim desmonta (mesmo se o
  `rsync` falhar, via `MacOsUnmounter`). Troca **no lugar** o `.app` em execução (sem staging atômico). Retorna
  `Ok(None)`, então reabre o mesmo caminho. Não há `codesign --verify`, `spctl` nem remoção de quarentena no
  arquivo.
- **CONFIRMADO - Linux** (`install_release_linux`, linhas 1129-1195): `tar -xzf` para um diretório temporário e
  `rsync -av --delete <zed.app> <destino>`. O destino é `~/.local`, ou o prefixo derivado do caminho do executável
  em execução quando ele termina em `zed{-canal}.app/libexec/zed-editor`. Instala sempre no espaço do usuário,
  **sem root e sem `.deb`/`.rpm`**. Exige `rsync` (`check_dependencies`, linhas 904-921) e dá dica por gerenciador de
  pacotes (`linux_rsync_install_hint`, linhas 51-98).
- **CONFIRMADO - Windows** (`install_release_windows`, linhas 1313-1333): roda o instalador baixado com
  `/verysilent /update=true /MERGETASKS=!desktopicon` e devolve o caminho de `tools/auto_update_helper.exe` como
  caminho de reinício. `finalize_auto_update_on_quit` (linhas 1335-1360): se existir `updates/versions.txt` (flag
  criada pelo instalador), no encerramento do app roda `auto_update_helper.exe --launch false`. O helper copia os
  binários novos, apaga os antigos e relança. `cleanup_windows` remove `updates`, `install` e `old` no próximo
  início (linhas 1299-1311). O código do helper (`crates/auto_update_helper`) **NÃO foi lido**. O comportamento de
  troca em dois estágios é inferido pelos comentários.
- **CONFIRMADO** - Reinício: após instalar, `cx.set_restart_path(new_binary_path)` (linha 854). O `on_app_restart` e
  o `on_app_quit` fazem o resto (linhas 437-460).

### UI

- **CONFIRMADO** - `AutoUpdateStatus`: `Idle | Checking | Downloading{version, progress} | Installing{version} |
  Updated{version} | Errored{error}` (linhas 123-139). A ação `Check` (manual) emite `UpToDate`. Existem
  `DismissMessage` e `ViewReleaseNotes` (linhas 100-110).
- **CONFIRMADO** - `auto_update_ui.rs`: depois de instalar, grava uma chave `SHOULD_SHOW_UPDATE_NOTIFICATION` no
  KVP. No próximo início mostra a notificação "Updated to {app} {version}" com botão para as notas da versão
  (linhas 302-370). As notas vêm de `/api/release_notes/v2/...` (linha 110) ou abrem a URL do navegador.
- **NÃO CONFIRMADO**: onde o `AutoUpdateStatus` aparece (provavelmente a barra de atividade do workspace, crate
  `activity_indicator`). Não li esse crate.

### Settings e gerenciador de pacote

- **CONFIRMADO** - `auto_update: bool`, padrão `true` (`docs/src/reference/all-settings.md` linhas 534-545).
  `AutoUpdateSetting` observa o `SettingsStore` e liga/desliga o polling em tempo real (linhas 251-305).
- **CONFIRMADO** - Pacote externo: se `ZED_UPDATE_EXPLANATION` estiver definido em compilação (`option_env!`) ou em
  execução, o polling **nunca começa** (linhas 285-287). A ação manual `check` abre um prompt "Zed was installed
  via a package manager." com o texto (linhas 310-322). A documentação de empacotamento recomenda isso
  (`docs/src/development/linux.md` linha 73). Não existe detecção automática do gerenciador. Quem empacota
  compila com a variável.

## VS Code

Fontes: `src/vs/platform/update/` em https://github.com/microsoft/vscode (branch `main`, 2026-11-23).

### Estrutura e endpoint

- **CONFIRMADO** - `AbstractUpdateService` + `UpdateService` por SO (`updateService.win32.ts`, `.darwin.ts`,
  `.linux.ts`). Máquina de estados em `common/update.ts` (`StateType`: `Idle`, `CheckingForUpdates`,
  `AvailableForDownload`, `Downloading`, `Downloaded`, `Updating`, `Ready`, `Restarting`, `Overwriting`, `Disabled`).
- **CONFIRMADO** - URL (`abstractUpdateService.ts` linhas 33-42):
  `new URL(`${baseUpdateUrl}/api/update/${platform}/${quality}/${commit}`)` com `bg=true` quando a checagem é em
  segundo plano e `u=<org interna|none>`. A `platform` é `win32-<arch>[-user|-archive]` no Windows
  (`updateService.win32.ts` linhas 208-218) e `linux-<arch>` no Linux. O host real (`update.code.visualstudio.com`)
  vem do `product.json` (`productService.updateUrl`) e **não aparece no código**, só o campo. Isso é **NÃO
  CONFIRMADO** na fonte lida.
- **CONFIRMADO** - Agendamento: primeira checagem após 30 s e depois a cada 60 min (`scheduleCheckForUpdates(delay =
  60 * 60 * 1000, repeat = true)`, linhas 313-340 e 449-460). Pula a checagem automática em conexão limitada
  (`IMeteredConnectionService`, linha ~477).

### `update.mode`

- **CONFIRMADO** (`common/update.config.contribution.ts` linhas 19-31): enum `none | manual | start | default`,
  padrão `default`, escopo `APPLICATION`. `none` desativa. `manual` desliga a checagem automática. `start` checa só
  na abertura. `default` checa na abertura e periodicamente. O serviço reaplica a configuração em tempo de
  execução (`reconfigure`, linhas 214-300).
- **CONFIRMADO** - Windows: existe `update.enableWindowsBackgroundUpdates` (padrão `true`, "baixar e instalar em
  segundo plano", só Windows, linhas 66-73). Existe também `update.enableEarlyStaging` (linha 74).

### Windows (`updateService.win32.ts`)

- **CONFIRMADO** - Download: usa `fileService.writeFile` para um `.tmp` e confere
  `update.sha256hash ? checksum(downloadPath, update.sha256hash) : undefined` (linha 313) antes de renomear para
  `CodeSetup-{quality}-{version}.exe` no `cachePath` em `tmpdir()` (linhas 66 e 384). **O hash só é conferido
  quando o servidor o envia**, ou seja, é opcional no cliente. Não vi checagem de assinatura própria, só a
  assinatura Authenticode do instalador pelo Windows.
- **CONFIRMADO** - Mutexes nomeados: `${win32MutexName}-ready`, `-updating` e `setup` (linhas 61-63 e 95-97),
  via `@vscode/windows-mutex`. O estado `Ready` avança quando o mutex `-ready` está ativo (linhas 474-480).
  Se já houver um setup rodando, o app **não** dispara outro instalador (evita o modal "Setup is already
  running", linhas 421-427).
- **CONFIRMADO** - Aplicação em segundo plano (`doApplyUpdate`, linhas 400-460): cria `CodeSetup-*.flag` e
  roda o instalador destacado com
  `/verysilent /log /update="<flag>" /progress="<arquivo>" /sessionend="<flag>" /cancel="<flag>"
  /nocloseapplications /mergetasks=runcode,!desktopicon,!quicklaunchicon`, e
  `env.__COMPAT_LAYER = RunAsInvoker` (evita pedir elevação). Faz polling do progresso por até 1 h. O instalador
  prepara os arquivos com o app aberto e **espera** o flag/mutex `-ready` antes de substituir. Quando o usuário
  fecha o app, `doQuitAndInstall` apaga o `.flag` (sinal para o instalador concluir) e grava um arquivo de
  argumentos de relançamento (`/relaunchargs`) para preservar `--extensions-dir` e similares (linhas 614-660).
- **CONFIRMADO** - Sem background update: `quitAndInstall` roda o instalador com
  `/silent /log /mergetasks=runcode,!desktopicon,!quicklaunchicon /relaunchargs=...` (linhas 637-650).
- **CONFIRMADO** - Se a instalação é `user` e o app roda como administrador, as atualizações são desativadas
  (`DisablementReason.RunningAsAdmin`, linhas 138-143).
- **CONFIRMADO** - `inno_updater.exe` existe em `resources/app/.../tools` e é usado na coleta de lixo
  (`collectGarbage`, linhas ~185-205). Seu papel completo na troca de arquivos **NÃO CONFIRMADO** (não li o
  repositório `Microsoft/inno-updater`).

### macOS (`updateService.darwin.ts`)

- **CONFIRMADO** - Delega ao `electron.autoUpdater`: `setFeedURL({url, headers})`, `checkForUpdates()`,
  `quitAndInstall()`, eventos `update-available`, `update-downloaded`, `update-not-available`, `error` (linhas
  30-50). O Squirrel.Mac baixa o `.zip`, valida e deixa pronto. A troca ocorre ao relançar via ShipIt (menção em
  comentário, linha 252). `SquirrelMacEnableEarlyStaging` é usado pelo `update.enableEarlyStaging` (linha 46).
- **NÃO CONFIRMADO**: como o Squirrel.Mac valida o pacote (a exigência de assinatura de código com o mesmo
  designated requirement é conhecimento geral, não lido nesta pesquisa).

### Linux (`updateService.linux.ts`, 89 linhas)

- **CONFIRMADO** - Só consulta `linux-<arch>` e coloca o estado em `AvailableForDownload`. `doDownloadUpdate` chama
  `nativeHostMainService.openExternal(downloadUrl)` e volta para `Idle(UpdateType.Archive)`. O comentário diz:
  "we don't currently detect the package type that was installed and the website download page is more useful than
  the tarball generally". **Não instala nada.** Quem instala via repositório apt/yum ou snap atualiza por eles.

### "Aplicar no próximo reinício"

- **CONFIRMADO** - No Windows, o `Ready` significa "arquivos preparados, falta o encerramento". `quitAndInstall` só
  libera o instalador. Veja acima. Nos modos `default`/`start` o app aplica sozinho na próxima abertura se o estado
  `Ready` já existir (`abstractUpdateService.ts` linhas 343-375, não lidas na íntegra).

## Outras ferramentas

### Tauri updater (v2)

Fonte: https://github.com/tauri-apps/tauri-docs (`src/content/docs/plugin/updater.mdx`).

- **CONFIRMADO** - Assinatura obrigatória: "needs a signature to verify that the update is from a trusted source.
  This cannot be disabled." (linha 84). Chave pública no `tauri.conf.json`. A privada via
  `TAURI_SIGNING_PRIVATE_KEY`. "if you lose this key you will NOT be able to publish new updates to the users that
  have the app already installed" (linha 89).
- **CONFIRMADO** - `endpoints`: TLS é imposto em produção e só passa ao próximo endpoint em resposta não-2XX
  (linha 205). `dangerousInsecureTransportProtocol` existe e é desencorajado.
- **CONFIRMADO** - JSON estático: `version`, `platforms.<os-arch>.url` e `.signature` obrigatórios; `signature` é o
  conteúdo do `.sig`, não URL (linhas 269-277).
- **CONFIRMADO** - Downgrade: por padrão só atualiza se `update.version > current`. `version_comparator` permite
  trocar (linhas 705-730).
- **CONFIRMADO** - Rotação: o `pubkey` pode ser definido em tempo de execução "useful to implement a key rotation
  logic" (linha 642). O Tauri não traz rotação pronta, só o gancho.
- **CONFIRMADO** - Windows `installMode`: `passive` (padrão, janela mínima), `basicUi`, `quiet`. Em `quiet` o
  instalador não pede elevação, então só serve para instalação por usuário ou app já elevado (linhas 236-255).
  Isso valida o nosso problema do `/ALLUSERS` silencioso.

### Sparkle e WinSparkle

- **CONFIRMADO** (WinSparkle README linhas 64-80): assinaturas EdDSA/Ed25519 compatíveis com o Sparkle, chave
  pública no app, `sparkle:edSignature` no appcast, "if you lose it [chave privada], you will not be able to issue
  any new updates". DSA legado deprecado, com migração em etapas.
- **CONFIRMADO** (https://sparkle-project.org/documentation/security-and-reliability/): o histórico de
  vulnerabilidades do Sparkle é um catálogo de erros comuns:
  - 2.6.4: "attacker to replace an existing signed update with another payload, which bypasses Sparkle's (Ed)DSA
    signing checks" (#2550).
  - 2.9.6: escalada local de privilégio em daemons que rodam como root.
  - 2.7.3: execução arbitrária de pacote via Installer XPC e condição de corrida.
  - 2.2.2: passar a chave privada na linha de comando é inseguro.
  - 2.5.2: corrupção rara que deixa o bundle sem arquivos e o Gatekeeper rejeita.
  - 2.6.4: escaneamento do Gatekeeper no app novo (macOS 14.4+) para evitar o diálogo "Verifying…" ao relançar.
- **CONFIRMADO** (https://sparkle-project.org/documentation/publishing/): `sparkle:version` numérica crescente
  separada de `sparkle:shortVersionString`.
- **NÃO CONFIRMADO** (só resumo de busca, não abri a nota): Sparkle 1.11.0 passou a exigir dupla verificação
  (assinatura Apple + EdDSA) e permite rotacionar entre as duas chaves se uma continuar válida.

### `self_update` (crate)

Fonte: https://github.com/jaemk/self_update (`README.md`, master).

- **CONFIRMADO** - Backends GitHub/GitLab/S3. Features `rustls`, `ureq` ou `reqwest`. `signatures` usa `zipsign` para
  `.zip` e `.tar.gz`. `checksums` verifica SHA-256/512 (linhas 54-91).
- **CONFIRMADO** - Substituição do binário em execução por `self_replace`. Modo "bundle" de diretório (macOS
  `.app`): extrai em staging **dentro do pai do destino** (mesmo filesystem), renomeia o executável em uso, tem hook
  `verify_binary` que recebe a raiz do bundle para `codesign --verify --deep`, e "never signs, notarizes, or
  staples" (linhas 269-290).
- **CONFIRMADO** - Lição macOS: um app em quarentena rodando em montagem de App Translocation (somente leitura)
  não consegue se atualizar no lugar. A crate detecta como `Error::AppTranslocated`. A saída é mover o app (o que
  limpa a quarentena) e relançar (linhas 287-290).
- **CONFIRMADO** - No Windows a troca falha (com rollback) se o processo mantiver arquivos do bundle abertos além do
  próprio executável. `.deb`/`.msi` são tratados fora da crate: "hand the downloaded file to `dpkg -i` / `msiexec /i`
  yourself" (linhas 292-297).

### cargo-dist / axoupdater

- **CONFIRMADO** (https://github.com/axodotdev/cargo-dist `book/src/installers/updater.md`): updater
  independente `<pacote>-update`, recurso "experimental", atualiza só instalações feitas por seus instaladores
  shell/PowerShell. Usa a API do GitHub sem autenticação (`AXOUPDATER_GITHUB_TOKEN` só para CI por causa do rate
  limit). Não cobre `.dmg`/`.deb`/`.rpm` nem assinatura própria (o texto não menciona). Pouco aproveitável para
  nós.

### minisign e TUF

- **CONFIRMADO** (https://github.com/jedisct1/minisign README): suporta *trusted comment* assinado
  (`minisign -S -m arquivo -t "comentário"`, linha ~138). É o lugar natural para colocar `version` e timestamp
  assinados.
- **CONFIRMADO** (https://github.com/theupdateframework/specification `tuf-spec.md`, linhas ~160-195): o TUF define
  ataques que um updater simples não trata: *endless data*, *indefinite freeze* (servir metadados antigos para o
  cliente nunca ver a versão nova), *mix-and-match*, *rollback*. Mitigações: metadados com data de expiração,
  números de versão monotônicos e **múltiplas chaves com limiar (threshold)** por papel para rotação e
  comprometimento parcial.
- **NÃO CONFIRMADO** (conhecimento geral): a assinatura minisign contém o ID da chave (8 bytes), o que permite ao
  cliente escolher entre várias chaves públicas embutidas.

## Comparação com o DB Modeling

| Aspecto | Zed | VS Code | Tauri/Sparkle | DB Modeling (spec/ADR) | Avaliação |
|---|---|---|---|---|---|
| Integridade | só TLS | `sha256hash` opcional | assinatura obrigatória | manifesto minisign + SHA-256 | Alinhado ao padrão forte, melhor que Zed/VS Code |
| Frequência | 1 h (nightly 15 min) | 30 s + 60 min | configurável | 10 s + 6 h com ETag | Alinhado |
| Canais | binário fixo por canal | `quality` no produto | endpoints | `update_channel` em runtime | Mais flexível, exige manifestos separados (ok) |
| Downgrade | `fetched > installed` | servidor decide por commit | `>` por padrão | semver maior + `min_supported_from` | Alinhado |
| Instala | no lugar (`rsync`/Inno+helper) | Inno em segundo plano com mutex | instalador de plataforma | Inno silencioso no encerramento; troca de `.app`; `pkexec` + pacote | Mais ambicioso no Linux que Zed e VS Code |
| Gerenciador externo | `ZED_UPDATE_EXPLANATION` | Linux só abre página | n/a | detecção por caminho/registro + `notify` | Alinhado, mais complexo (ver riscos) |
| Erros | silencioso no automático | idem (`explicit`) | n/a | silencioso no automático | Alinhado |
| Retomada | não | não confirmado | n/a | `Range` + `.part` | Além dos dois |
| Telemetria | opcional | `u=`/`bg=` | nenhuma | nenhuma | Alinhado e melhor |
| Rotação de chave | n/a | n/a | gancho `pubkey` | "versão-ponte" no ADR | Falta mecanismo (ver R1) |
| Aplicar no reinício | `set_restart_path`/quit hook | flag + mutex | `quitAndInstall` | "Depois" aplica na próxima abertura | Alinhado |

### O que está alinhado

- Checagem e download em segundo plano, indicador discreto, instalação no reinício, silêncio em falha automática.
- Assinatura Ed25519 sobre o manifesto, chave pública embutida, backup offline da privada (Tauri, WinSparkle e
  Sparkle dizem o mesmo).
- Recusa de downgrade e de manifesto de esquema desconhecido.
- Instalador de plataforma já assinado (SignPath/notarização) como artefato, sem patch binário (igual VS Code e Zed).
- `DBM_UPDATE=off` e modo `notify` para instalações externas (equivale a `ZED_UPDATE_EXPLANATION`).

### O que falta ou diverge

1. Sem proteção contra **freeze/replay** de manifesto (TUF). Manifesto antigo e válido passa na assinatura e o
   usuário nunca vê a atualização nova.
2. Sem **rotação** além de "versão-ponte" (uma chave só embutida).
3. Windows com `/CLOSEAPPLICATIONS /RESTARTAPPLICATIONS` enquanto o app roda: o VS Code optou por
   `/nocloseapplications` e controle próprio de relançamento. O Zed usa helper externo. Nosso desenho não diz quem
   espera o processo sair.
4. Sem trava de instância/instalação (VS Code usa mutex nomeado; Zed usa `pending_poll`).
5. Janela entre **verificar** e **instalar** (principalmente com `pkexec`) não é tratada.
6. macOS: sem verificação de assinatura do `.app` montado nem tratamento de App Translocation.

## Riscos e recomendações

Ordenado por impacto. Itens marcados (inferência) são análise do desenho, não afirmação das fontes.

**R1. Rotação de chave e chave perdida.**
- Embutir, desde a primeira versão com updater, **duas chaves públicas** (primária e de reserva, guardada offline em
  outro local) e aceitar assinatura de qualquer uma. Assim, perder ou vazar a primária não corta o canal de update.
  A "versão-ponte" do ADR continua útil, mas só funciona se alguém já atualizou para ela, o que a chave
  comprometida pode bloquear (inferência).
- Seguir o gancho do Tauri (`pubkey` em runtime) e o limiar de chaves do TUF como inspiração. Não implementar TUF
  completo.
- Documentar procedimento de revogação: lançar versão assinada pela reserva que embute nova reserva.
- Backup da privada sem senha em CI é risco. Usar senha na chave minisign e segredo separado (Sparkle depreciou
  passar chave em argumento de linha de comando: usar stdin/arquivo no `release.yml`, nunca `-s`).

**R2. Replay, freeze e downgrade do manifesto.**
- Colocar `version` e `pub_date` **dentro do conteúdo assinado** (já estão no JSON; reforçar com *trusted comment*
  minisign contendo `version` e timestamp, que a assinatura cobre).
- Adicionar `expires` (por exemplo 30-90 dias) ao manifesto e recusar manifestos expirados em checagem automática.
  Para manifesto ainda dentro do prazo mas com `version` menor que a última já vista, ignorar (guardar
  `update_last_seen_manifest_version`). Mitiga freeze/rollback (TUF).
- A escolha de `min_supported_from` impede saltos inválidos, mas não impede o servidor de **segurar** o cliente numa
  versão: por isso o `expires`.
- Verificar a assinatura sobre os **bytes exatos** baixados, **antes** de interpretar o JSON. Só depois fazer `serde`.

**R3. Condição de corrida entre verificar e instalar (TOCTOU), principalmente Linux com root.**
- O Sparkle teve CVEs de substituição do payload assinado (2.6.4) e escalada local via helper root (2.9.6). O
  mesmo vale para o nosso `pkexec apt install ./arquivo.deb`: o `.deb` fica em `<cache>` do usuário, que um processo
  do mesmo usuário pode trocar depois da verificação (inferência).
- Recomendação: antes do `pkexec`, copiar para um diretório criado com `0700` e reabrir pelo descritor, ou, melhor,
  rodar o `pkexec` com um **pequeno helper nosso** que recalcula o SHA-256 do arquivo já dentro do contexto root e
  compara com o valor assinado. Nunca passar caminho em diretório gravável por outros.
- Diretório de cache de update com permissão `0700` (Unix) e ACL do usuário (Windows). O Zed usa
  `tempfile::TempDir`. Nós também precisamos disso em `<cache>/db-modeling/updates/`.
- Reverificar o hash **imediatamente antes** do `spawn` do instalador (não só ao fim do download).

**R4. Windows: arquivo em uso e elevação.**
- Rodar o instalador **depois** que o processo do app saiu (espera por PID ou helper destacado). Se o Inno rodar
  com o app aberto, os `.dll`/`.exe` em uso falham ou exigem reinício. O VS Code resolve com `/nocloseapplications`,
  mutex/flag de "pronto" e arquivo de argumentos de relançamento. O Zed resolve com helper que troca os binários
  após o encerramento. Nosso `/CLOSEAPPLICATIONS /RESTARTAPPLICATIONS` depende do Restart Manager e
  **NÃO CONFIRMADO** que funcione com app GPUI sem janela padrão (testar).
- Recomendado: `/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /SP- /NOCANCEL` + `/CURRENTUSER` ou `/ALLUSERS`.
  Relançar pelo próprio app (ou por tarefa `[Run]` com `postinstall`), repassando os argumentos originais como o
  VS Code faz (`/relaunchargs`).
- O `quiet` do Tauri deixa claro: instalador silencioso **não** pede elevação sozinho. `/ALLUSERS` de um processo
  não elevado dispara UAC com a UI do app já fechada, o que parece falha. Recomendação: instalação por máquina
  sem elevação disponível fica em `notify`, ou o UAC acontece em "Reiniciar agora" (ação explícita), nunca no
  encerramento silencioso (inferência).
- Seguir o VS Code: desativar o update quando a instalação por usuário roda como administrador (confirmado em
  `updateService.win32.ts` linha ~140).
- Usar mutex nomeado para impedir dois apps/instaladores simultâneos e para o instalador esperar o app (o VS Code
  também evita disparar um segundo setup enquanto outro roda).
- Verificar a assinatura Authenticode (SignPath) do `.exe` antes de rodar é opcional e útil como defesa em
  profundidade. Não substitui o SHA-256 do manifesto.

**R5. macOS: Gatekeeper, quarentena, translocation e troca atômica.**
- **CONFIRMADO** no `self_update`: app em App Translocation não se atualiza no lugar. Tratar `Error` equivalente:
  detectar caminho `/private/var/folders/.../AppTranslocation/` e cair para "abra o `.dmg`" (já previsto no spec como
  fallback para destino sem permissão de escrita, mas inclua este caso).
- **Não copie em cima do `.app` em execução como o Zed** (`rsync --delete` no lugar). Faça staging na **mesma
  partição**, no pai do destino, valide e depois `rename` (o spec já pede staged swap, e o `self_update` confirma
  esse desenho: staging dentro do pai do destino, executável em uso renomeado antes).
- Antes de trocar, validar o bundle montado: `codesign --verify --deep --strict` e `spctl --assess --type execute`,
  e **comparar o Team ID** com o do app atual (o `self_update` expõe o hook `verify_binary` para isso). O Zed não
  faz essa checagem, o Sparkle faz (dupla verificação, **NÃO CONFIRMADO** na nota).
- Quarentena: arquivo baixado por `ureq` normalmente **não** recebe o atributo `com.apple.quarantine` (isso é
  conhecimento geral, **NÃO CONFIRMADO** nas fontes lidas). Mesmo assim o Sparkle 2.6.4 escaneia o app novo pelo
  Gatekeeper no macOS 14.4+ para evitar o diálogo "Verifying…". Em ambiente real, testar: notarizado + staple
  garante que o Gatekeeper abra offline. Se o `.dmg` for notarizado mas o app dentro não estiver com ticket
  grampeado, o primeiro relançamento depende de rede.
- Montar com `hdiutil attach -nobrowse -readonly -noverify -noautoopen`, desmontar sempre (como o `MacOsUnmounter`
  do Zed) e limpar o `.dmg` depois.
- Relançar com `open -n` do caminho novo, passando o PID antigo para esperar a saída.

**R6. Linux: `pkexec`, Wayland e gerenciadores.**
- Nem o Zed nem o VS Code instalam `.deb`/`.rpm` automaticamente. O Zed instala só no espaço do usuário (tarball
  + `rsync`), o VS Code abre a página de download. O nosso `pkexec` é o item de maior risco do projeto: começar
  com `notify` + botão "copiar comando" e liberar a instalação automática depois de teste manual em Ubuntu e
  Fedora (inferência).
- `pkexec` precisa de um agente polkit na sessão. Em Wayland funciona se o ambiente (GNOME/KDE) tiver agente, mas
  falha em sessões sem agente (SSH, WSL, WM mínimo). Detectar o erro e cair para `notify`. O `pkexec` limpa o
  ambiente: passar caminhos absolutos e não depender de `DISPLAY`/`XDG_*` (inferência).
- `apt install ./arquivo.deb` e `dnf install ./arquivo.rpm` resolvem dependências, mas o `apt` pode avisar de
  "unsandboxed download" quando o arquivo não é legível pelo usuário `_apt`. Copiar para diretório acessível e
  root-owned (item R3) resolve os dois problemas.
- `.rpm` local: a política de GPG do `dnf` para arquivos locais difere entre distribuições e **NÃO foi
  confirmada**. Se não assinarmos o `.rpm`, a segurança vem só do nosso manifesto. Assinar `.deb`/`.rpm` e/ou usar
  `--nogpgcheck` conscientemente.
- Detecção de gerenciador externo: preferir consultar o dono do arquivo (`dpkg -S`/`rpm -qf` do executável) em vez
  de só inspecionar o caminho. Se o app **foi** instalado pelo nosso `.deb`/`.rpm`, o `pkexec` é legítimo. Se foi
  Flatpak/AppImage/Snap/tarball, `notify`. O Zed resolve isso em compilação (`ZED_UPDATE_EXPLANATION`). Considerar
  uma variável equivalente nos nossos pacotes (por exemplo, o `.deb` grava um marcador em `/usr/share/db-modeling/`).

**R7. Download, hospedagem e redirecionamento.**
- A URL do manifesto no GitHub (`releases/latest/download/latest.json`) redireciona para um CDN. Aceitar
  redirecionamentos só entre hosts da allowlist (`github.com`, `objects.githubusercontent.com`,
  `release-assets.githubusercontent.com`) e **somente HTTPS**. Confirmar quais hosts a Release usa hoje antes de
  fixar (**NÃO CONFIRMADO**).
- O host da URL do artefato dentro do manifesto deve estar na allowlist, mas a segurança real vem da assinatura
  + SHA-256 (o Tauri exige TLS, mas o que valida é a assinatura).
- Para reduzir uso da API REST: baixar o asset `latest.json` diretamente (URL de download) em vez de chamar
  `api.github.com`. O limite anônimo da API se aplica à API, não ao download de assets (conhecimento geral,
  **NÃO CONFIRMADO** na documentação). O `ETag` continua útil.
- `releases/latest/download` ignora prereleases. Para o canal `prerelease`, usar um asset em release com tag fixa
  (por exemplo `updates-prerelease`) atualizado pelo workflow (inferência).
- Limitar tamanho do manifesto e do artefato (`Content-Length` + corte durante o streaming) e timeouts de conexão e
  de leitura. O Zed não faz nada disso. Isso evita *endless data* (TUF).
- Retomada com `Range`: guardar o hash do manifesto junto do `.part` para não misturar versões; se o servidor
  ignorar `Range`, recomeçar.

**R8. Estado e concorrência.**
- Mutex/lock de arquivo por usuário para impedir duas instâncias do app baixando/instalando juntas (VS Code usa
  mutex nomeado, Zed protege com `pending_poll` só dentro do processo).
- Não reiniciar fases de instalação após suspensão (o Zed só reexecuta fases de rede, linhas 474-488).
- Pular checagem em conexão limitada (VS Code já faz).
- Limpar instaladores velhos no início (Zed: 24 h; spec: manter só o mais recente: ok).

**R9. Notas, links e dados do manifesto.**
- `notes_url` e `url` vêm de dado assinado, mas ainda assim validar esquema `https` e host antes de abrir no
  navegador.

### As 5 recomendações mais importantes para o spec

1. Embutir **duas chaves públicas** (primária + reserva offline) e aceitar qualquer uma (R1).
2. Colocar `version`/timestamp no *trusted comment* minisign, adicionar `expires` ao manifesto e guardar a maior
   versão de manifesto já vista, contra freeze/rollback (R2).
3. Eliminar o TOCTOU: diretório `0700`, reverificar o SHA-256 imediatamente antes do instalador e, no `pkexec`, um
   helper que recalcula o hash dentro do contexto root, em diretório root-owned (R3, R6).
4. Windows: executar o instalador **depois** da saída do processo, com mutex, `/NORESTART`, relançamento controlado
   pelo app e política explícita para instalação por máquina (UAC só em ação explícita) (R4).
5. macOS: staging no pai do destino + `rename`, `codesign --verify --deep --strict` e conferência de Team ID antes
   da troca, e fallback para App Translocation. Linux `.deb`/`.rpm`: liberar a instalação automática só depois de
   teste manual (começar em `notify`) (R5, R6).

## Zed em detalhe

Decisão do mantenedor: o Zed é o guia do auto-update do DB Modeling. Esta seção aprofunda só o Zed (estava no arquivo
`auto-update-desktop-zed-detalhe.md`, já fundido aqui). Arquivos lidos (commit `6501df15765d66d6c2a2b66b1eb22434ccef3f86`,
ver o topo; linhas da cópia lida em 2026-11-23): `crates/auto_update/src/auto_update.rs` (AU),
`crates/auto_update_helper/src/{auto_update_helper,updater,dialog}.rs` (HLP), `crates/zed/resources/windows/zed.iss` (ISS),
`script/bundle-windows.ps1`, `script/bundle-linux`, `script/bundle-mac`, `script/install.sh`,
`crates/title_bar/src/{title_bar,update_version}.rs`, `crates/ui/src/components/collab/update_button.rs`,
`crates/zed/src/main.rs`, `crates/zed/src/zed/app_menus.rs`. URL base:
https://github.com/zed-industries/zed/blob/6501df15765d66d6c2a2b66b1eb22434ccef3f86/<arquivo>.

### 1. Windows: `auto_update_helper` e o Inno

- **CONFIRMADO - empacotamento** (`script/bundle-windows.ps1` linhas 119-122, 223 e 250): o helper é uma crate separada
  (`--package auto_update_helper`), assinada junto com `Zed.exe` e movida para `tools\auto_update_helper.exe`. O `.iss`
  instala `tools\*` direto em `{app}\tools` (ISS linha 70), nunca em `install\`.
- **CONFIRMADO - o que o Inno faz com `/update=true`** (ISS linhas 1389-1422): `IsUpdating()` = switch `update=true` **e**
  `WizardSilent()`. Nesse modo: (a) `GetInstallDir` devolve `{app}\install`, então `Zed.exe`, `bin\*`, `OpenConsole.exe` e
  `conpty.dll` vão para `{app}\install\...` e **não sobrescrevem os arquivos em uso** (linhas 67-80); (b) `GetAppMutex`
  devolve `''`, ou seja, o instalador **não espera o app fechar** (linhas 1407-1414); (c) no `ssPostInstall` grava
  `{app}\updates\versions.txt` com a versão (`SaveStringToFile`, linha 1402). Esse arquivo é só a **flag** de "instalação
  pronta": o cliente só testa a existência (`flag_file.exists()`, AU linha 1345). Também `PrivilegesRequired=lowest`
  (ISS linha 39) e `CloseApplications=force` (linha 32). A linha de comando do updater **não** passa `/ALLUSERS` nem
  `/CURRENTUSER` (AU linha 1315). Como uma instalação por máquina se comporta (elevação) **NÃO CONFIRMADO**: o Zed trata
  tudo como instalação por usuário.
- **CONFIRMADO - quando o helper roda**: (1) ao **reiniciar** após atualizar: `install_release_windows` devolve
  `tools\auto_update_helper.exe` e o AU faz `cx.set_restart_path(helper)` (AU linhas 854 e 1313-1333), então o "reinício"
  executa o helper, que troca os arquivos e relança `Zed.exe`; (2) ao **sair** sem reiniciar: o hook `on_app_quit` chama
  `finalize_auto_update_on_quit`, que, se `updates\versions.txt` existe, roda `tools\auto_update_helper.exe --launch false` e
  **espera o fim** (`cmd.status().await`, AU linhas 1335-1360). No reinício, `on_app_restart` solta a subscription de quit
  para o helper não rodar duas vezes (AU linhas 447-449).
- **CONFIRMADO - argumentos do helper** (HLP linhas 95-115): `--launch true|false` ou `--launch=false`; qualquer outro
  argumento vira `launch_arguments`, repassado ao `Zed.exe` novo (o teste preserva `--user-data-dir "C:\Zed Data"`).
  Padrão `launch = true`.
- **CONFIRMADO - o helper em si** (HLP linhas 49-81): subsistema `windows` (sem console), log em
  `tools\auto_update_helper.log` (append), `app_dir` = pai de `tools`. Abre uma janela própria "Updating Zed..."
  (`dialog.rs` linha 174) com barra de progresso. Uma thread roda `perform_update` e posta `WM_JOB_UPDATED` a cada job.
  Erro vira `MessageBoxW` "Error: Zed update failed." truncado em 600 caracteres.
- **CONFIRMADO - ordem das operações** (`updater.rs` linhas 167-214, 22 jobs, cada um com `apply` e `rollback`):
  1. `mkdir old`; mover `Zed.exe`, `bin\Zed.exe`, `bin\zed`, `conpty.dll` e `OpenConsole.exe` (x64/arm64, se existirem)
     para `old\` ("Not deleting because installing new files can fail").
  2. Mover de `install\` para o lugar: `Zed.exe`, `bin\Zed.exe`, `bin\zed`, `OpenConsole.exe` e `conpty.dll`.
  3. Apagar `updates`, `install` e `old` (`rmdir_nofail`, sem rollback; falha só gera warning).
- **CONFIRMADO - arquivo em uso e retentativa** (linhas 274-364 e 386-418): antes dos jobs tenta liberar handles com o
  **Restart Manager** (`RmStartSession`, `RmRegisterResources` nos 4 arquivos, `RmShutdown(flags 0)`), melhor esforço.
  Cada job tem **timeout de 2 s** e, em `io::Error` que não seja `NotFound`, repete a cada 50 ms. `NotFound` ou erro
  não-IO aborta.
- **CONFIRMADO - rollback** (linhas 420-439): se um job falha, executa o `rollback` dos já concluídos em ordem inversa
  (mover de volta). Erros possíveis: "Autoupdate failed, rollback successful", "nothing to rollback" ou "Job rollback
  failed, the app might be left in an inconsistent state". Em falha **não relança** nada (o `launch` só ocorre em sucesso,
  linhas 441-446).
- **CONFIRMADO - relançamento**: `Command::new(app_dir\Zed.exe).args(launch_arguments).spawn()` (linhas 366-371 e 441-446).
- **CONFIRMADO - limpeza no próximo início**: `cleanup_windows` remove `updates`, `install` e `old` com `remove_dir`
  (só se vazios, AU linhas 1299-1311), no início do polling (linhas 498-505).
- **CONFIRMADO - onde o instalador baixado fica**: no Windows é `<dir do Zed.exe>\updates` (recriado, AU linhas 393-410),
  não `%TEMP%`. Exige permissão de escrita no diretório de instalação.
- **Sem mutex entre app e helper**: a sincronização é só "o app já saiu" (quit hook ou restart). O Inno não toca nos
  arquivos vivos porque instala em `install\`.

### 2. Linux

- **CONFIRMADO - nome do asset**: o bundler gera `zed-linux-${arch}.tar.gz` (`script/bundle-linux` linha 247) contendo a
  pasta `zed$suffix.app` (`suffix=-<canal>` se não for stable, linhas 178-186 e 252). O cliente grava como `zed.tar.gz`
  (AU linha 926) e pede `asset=zed`.
- **CONFIRMADO - layout interno**: `zed.app/bin/zed`, `libexec/zed-editor`, `share/icons/hicolor/512x512/apps/zed.png`,
  `1024x1024/apps/zed.png` e `share/applications/<APP_ID>.desktop` (bundle-linux linhas 190-240).
- **CONFIRMADO - extração e destino** (AU linhas 1129-1195): `tar -xzf zed.tar.gz -C <tmp>/zed`, depois
  `rsync -av --delete <tmp>/zed/zed$suffix.app <destino>`. Destino padrão `~/.local` (resulta em `~/.local/zed$suffix.app`).
  Se o executável em execução termina em `zed$suffix.app/libexec/zed-editor`, o destino vira **o prefixo desse caminho**
  (instalação em outro lugar é preservada). O novo executável é devolvido como restart path.
- **CONFIRMADO - .desktop e ícones**: o **updater não toca** no `.desktop`, nos ícones nem no symlink `~/.local/bin/zed`.
  Quem cria é o `install.sh` na primeira instalação (linhas 130-150: `ln -sf`, copia o `.desktop` e faz `sed` em `Icon=` e
  `Exec=` com caminho absoluto). Como o caminho não muda, o `.desktop` continua válido.
- **CONFIRMADO - primeira instalação**: `install.sh` usa
  `https://cloud.zed.dev/releases/$channel/$ZED_VERSION/download?asset=zed&arch=$arch&os=linux&source=install.sh`
  (linha 86) e `tar -xzf ... -C ~/.local/` (linha 117). **Isto confirma o host `cloud.zed.dev`** (reconferido no commit
  `6501df1`, linha 86).
- **CONFIRMADO - sem gerenciador de pacote e sem elevação**: o Zed no Linux **nunca usa root, `pkexec`, `.deb` ou `.rpm`**.
  Pasta sem permissão de escrita: nenhum tratamento especial; o `rsync` falha e o erro vira `Errored` (manual) ou silêncio
  (automático), com texto "failed to copy Zed update from ... to ...".
- **CONFIRMADO - instalação por gerenciador**: `ZED_UPDATE_EXPLANATION` em compilação (`option_env!`) ou em execução
  (AU linhas 285-287 e 310-322). Presente: o polling nunca inicia e a ação `Check` abre o prompt "Zed was installed via a
  package manager." com a mensagem. Não existe detecção automática.
- **CONFIRMADO - dependência**: `rsync` obrigatório (`check_dependencies`, AU linhas 904-921) com dica de instalação por
  distro. `MissingDependencyError` é o único erro automático que chega à UI.

### 3. macOS

Sequência de `install_release_macos` (AU linhas 1197-1250), **CONFIRMADO**:
1. Pasta temporária `tempfile::Builder().prefix("zed-auto-update")` em `$TMPDIR` (linhas 372-391); o `.dmg` baixado fica nela.
2. `hdiutil attach -nobrowse <dmg> -mountroot <tmp>`. O volume se chama `Zed` (`hdiutil create -volname Zed`,
   `script/bundle-mac` linha 298), então monta em `<tmp>/Zed`.
3. `rsync -av --delete --exclude "Icon?" <tmp>/Zed/<NomeDoApp>.app/ <app em execução>`. O nome do `.app` vem do app **em
   execução**. É cópia **no lugar** sobre o `.app` ativo.
4. `hdiutil detach -force <mount>` sempre, mesmo se o `rsync` falhar (`MacOsUnmounter`, linhas 198-248, com `Drop` como rede
   de segurança). Só depois o diretório temporário é apagado (senão o dmg montado impede e o diretório vaza).
5. Devolve `Ok(None)`: sem `set_restart_path`; o reinício reabre o mesmo caminho (via `workspace::reload`, ver §5).

- Erros de `attach` e `rsync` viram `anyhow` com `stderr`. **Não há** `codesign`, `spctl`, remoção de quarentena nem
  staging atômico. O dmg é assinado e notarizado no build (`bundle-mac` linhas 241-311), mas o cliente não verifica nada.
  Diretórios `zed-auto-update*` com mais de 24 h são apagados (AU linhas 1253-1295).
- **NÃO CONFIRMADO**: se o dmg baixado herda a quarentena (não vimos código que a grave nem que a remova).

### 4. Ciclo de vida

- **CONFIRMADO - início**: `auto_update::init(client, cx)` em `main.rs` linha 669 e `auto_update_ui::init(cx)` na 671.
  `AutoUpdater::new` só registra hooks de quit/restart/wake. Quem liga o polling é o `init` (AU linhas 277-307): exige
  `poll_for_updates` (não `Dev`), `ZED_UPDATE_EXPLANATION` ausente e `auto_update = true`.
- **CONFIRMADO - sem atraso inicial**: `start_polling` faz `poll(Automatic)` **imediatamente** e só depois dorme
  `POLL_INTERVAL` (1 h; nightly 15 min) (AU linhas 507-515). O Zed não tem os "10 s de atraso" do desenho inicial.
- **CONFIRMADO - ciclo de `update()`** (AU linhas 740-866): `check_dependencies` -> `Checking` -> `get_release_asset` ->
  compara versão -> `Downloading{version, progress}` -> baixa -> `Installing{version}` -> `install_release` em
  `background_spawn` -> `set_restart_path` (se houver) -> grava `SHOULD_SHOW_UPDATE_NOTIFICATION` no KVP -> `Updated{version}`.
  Não existe estado "pronto mas não instalado": **o Zed instala os arquivos assim que baixa** (macOS/Linux sobrescrevem o
  app em execução; Windows prepara em `install\`). `Updated` significa "reinicie para usar".
- **CONFIRMADO - aplicação**: o binário novo só roda depois de reiniciar (botão "Restart to Update" chama
  `workspace::reload`). No Windows, sair do app também finaliza (hook de quit). No macOS/Linux, sair e abrir manualmente já
  abre a versão nova, porque os arquivos já foram trocados. Não há etapa de "aplicar" na abertura seguinte.
- **CONFIRMADO - repetição**: no estado `Updated`, a próxima checagem compara com a versão **já instalada pelo update**
  (`Updated{version}`), não com a do processo, para não baixar de novo (AU linhas 868-902). Sem novidade, preserva `Updated`.
- **CONFIRMADO - comparação**: stable/preview: `fetched > installed` com `pre` e `build` zerados. Nightly: compara o último
  segmento de `build` (SHA do commit) com o `AppCommitSha` (linhas 878-886 e 960-967).
- **CONFIRMADO - retentativas**: não há retentativa nem backoff. Falha automática vira `Idle`; a próxima tentativa é a da
  próxima hora. Após suspensão, `restart_after_wake` reinicia só `Checking`/`Downloading` (linhas 474-488). Só um
  `pending_poll` por vez; um `poll` manual durante um automático só muda o tipo (linhas 521-532).
- **CONFIRMADO - cache e limpeza**: sem cache persistente nem retomada. Cada tentativa cria um diretório novo e o apaga no
  fim (`TempDir`). Órfãos com mais de 24 h são removidos no início do polling (não Windows). No Windows, `updates`,
  `install` e `old` são removidos por `cleanup_windows` e pelos jobs do helper.
- **CONFIRMADO - download**: `http_client.get(url, Default::default(), true)` (o `true` segue redirecionamentos), falha se o
  status não for sucesso, progresso por `Content-Length`, sem `Range` (AU linhas 1076-1127).

### 5. UI

- **CONFIRMADO - local**: **barra de título** (title bar), não a barra de status. `title_bar.rs` linha 404 faz
  `.child(self.update_version.clone())`, e `activity_indicator.rs` não referencia o updater. O componente `UpdateVersion`
  (`update_version.rs`) renderiza um `UpdateButton` (crate `ui`).
- **CONFIRMADO - textos** (`update_button.rs` linhas 103-139): "Checking for Zed Updates…" (só se manual), "Downloading Zed
  Update…" (tooltip "Update to Version: X (N% downloaded)"), "Installing Zed Update…", "Up to Date" (some após
  `UP_TO_DATE_DISPLAY_DURATION = 4 s`), **"Restart to Update"** (tooltip da versão, botão "Dismiss") e "Failed to Update"
  (tooltip com o erro; o clique abre o log via `workspace::OpenLog`; dispensável).
- **CONFIRMADO - botão**: o clique em "Restart to Update" chama `workspace::reload(cx)`. Dispensar guarda `dismissed_status`
  e esconde o botão; `show_update_in_menu_bar` mostra o item no menu quando dispensado e `Updated`.
- **CONFIRMADO - menu e ações**: `MenuItem::action("Check for Updates", auto_update::Check)` (`app_menus.rs` linha 75) e
  `auto_update_ui::ViewReleaseNotesLocally` (linha 309). `actions!(auto_update, [Check, DismissMessage, ViewReleaseNotes])`
  (AU linhas 100-110). O texto literal na command palette (algo como "auto update: check") **NÃO CONFIRMADO**, não
  executei o app.
- **CONFIRMADO - notificação pós-update**: depois de reiniciar, `show_update_notification` mostra "Updated to {app}
  {version}" com ação para as notas (`auto_update_ui.rs` linhas 302-370). A flag no KVP é gravada ao concluir a
  instalação e apagada ao exibir. As notas vêm de `/api/release_notes/v2/{canal}/{versão}` num buffer markdown local, ou
  abrem a URL no navegador (linhas 89-190).
- **CONFIRMADO - setting**: `auto_update` (bool, padrão `true`), setting de usuário observada em tempo real
  (`docs/src/reference/all-settings.md` linhas 534-545; AU linhas 251-305). Override por projeto **NÃO CONFIRMADO**.
- **CONFIRMADO - tela de colaboração**: com `UpgradeRequired` o botão do title bar vira "Please restart Zed to Collaborate" /
  "Updating..." / "Please update Zed to Collaborate" (`title_bar.rs` linhas 1185-1196). Sem equivalente nosso.

### 6. Canal, endpoint, headers e integridade

- **CONFIRMADO - endpoint**: `GET {cloud}/releases/{canal}/{versão|latest}/asset?asset=zed&os={os}&arch={arch}` (AU linhas
  677-740), com `os` e `arch` de `std::env::consts`. Host `cloud.zed.dev` (confirmado pelo `install.sh`, que usa a rota
  `/download`; a rota `/asset` do updater só tem o caminho confirmado no código).
- **CONFIRMADO - resposta**: JSON `{"version": "<semver>", "url": "<https...>"}` (`ReleaseAsset`, AU linhas 193-196). Sem
  hash, assinatura, tamanho nem data.
- **CONFIRMADO - canal**: `ReleaseChannel::global(cx).dev_name()` (`dev`, `nightly`, `preview`, `stable`), compilado no
  binário (`crates/zed/RELEASE_CHANNEL`, lido pelo `bundle-linux` linha 90). Não é setting.
- **CONFIRMADO - privacidade**: a query inclui `metrics_id`, `system_id` e `is_staff` **somente** se
  `telemetry().metrics_enabled()` (AU linhas 686-695). Outros headers (User-Agent etc.) **NÃO CONFIRMADO**.
- **CONFIRMADO - integridade**: **nenhuma verificação** em qualquer ponto do cliente (sem SHA-256, assinatura ou Team ID).
  Só TLS e a assinatura de plataforma aplicada pelo SO ao executar.

### O que o spec pode copiar 1:1 do Zed

1. UX da title bar: botão único com `Checking` (só manual) / `Downloading` com % / `Installing` / `Up to Date` (4 s) /
   **Restart to Update** (tooltip "Update to Version: X" e "Dismiss") / `Failed to Update` (clique abre o log).
2. Máquina de estados `Idle|Checking|Downloading{version,progress}|Installing{version}|Updated{version}|Errored`: erro
   silencioso no automático e visível no manual; um `pending_poll` por vez; manual sobrepõe automático; comparar com a
   versão **já instalada pelo update** para não baixar de novo.
3. Tipos `Automatic`/`Manual` e evento `UpToDate`.
4. Setting booleana `auto_update` aplicada em tempo real e override por ambiente tipo `ZED_UPDATE_EXPLANATION` (nosso
   `DBM_UPDATE_EXPLANATION`) com mensagem ao usuário.
5. Notificação pós-update por flag persistida ("Updated to X" + notas), equivalente ao nosso `last_seen_version`.
6. Reiniciar só fases de rede após `system wake`.
7. Limpeza de diretórios órfãos (> 24 h) e `hdiutil detach -force` sempre, inclusive por `Drop`.
8. Windows: Inno instala em `install\` sem esperar o app (`AppMutex` vazio), grava `updates\versions.txt`, hook de quit
   roda o helper com `--launch false`, restart roda o helper, Restart Manager antes, jobs `apply`/`rollback`, timeout de
   2 s com retry de 50 ms, repasse de argumentos ao relançar, log do helper e diálogo "Updating…".
9. Linux: tarball em `~/.local/<app>.app` com `rsync -av --delete`, destino derivado do caminho do executável, e **não
   tocar** em `.desktop`/ícones/symlink no update.
10. macOS: `hdiutil attach -nobrowse ... -mountroot <tmp>`, `rsync -av --delete` do `.app`, `detach -force`.

### O que NÃO dá para copiar (e por quê)

Escrito antes da decisão final; o resultado de cada ponto está em [docs/auto-update.md](../auto-update.md).

- **Ausência de verificação**: o Zed confia em TLS e no servidor próprio. Nosso download vem de uma Release do GitHub.
  Manter manifesto assinado + SHA-256 (ADR-0010). É o desvio mais importante e justificado.
- **Endpoint dinâmico**: o Zed tem servidor que escolhe o artefato por `os/arch`. Nós só temos arquivos estáticos, então o
  `latest.json` com `platforms` substitui o endpoint.
- **Sem atraso inicial e sem rate limit**: o Zed consulta no primeiro frame. Com o GitHub, o desenho inicial sugeria atraso
  de 10 s, `ETag` e 6 h (inferência). **Decisão final:** primeira checagem imediata e intervalo de 1 h, como o Zed, com `ETag`.
- **Instalar tudo ao baixar**: o Zed sobrescreve o `.app` em execução no macOS/Linux assim que baixa. O desenho inicial
  mandava "baixar em segundo plano, instalar no próximo reinício". A cópia no lugar do Zed, sem staging nem verificação,
  deixa o app quebrado se faltar energia no meio do `rsync --delete` (inferência). **Decisão final:** copiar o Zed (instala
  na hora) e aceitar o risco do `rsync` interrompido. O `codesign` e o Team ID conferem o artefato **antes** da cópia,
  mas não evitam a inconsistência se a cópia for interrompida.
- **Linux só com tarball**: o Zed distribui `tar.gz` em `~/.local`, sem root. Nosso release gerava só `.deb`/`.rpm`.
  **Decisão do mantenedor:** tarball de usuário (copia o Zed 1:1); `.deb`/`.rpm` continuam sem auto-update.
- **Windows sempre por usuário**: o Zed só atualiza onde o usuário escreve em `{app}`. **Decisão final:** igual, sem UAC;
  instalação por máquina só mostra a explicação.
- **Helper e Inno**: copiar exige uma crate `auto_update_helper` (~700 linhas entre `updater.rs` e `dialog.rs`), o crate
  `windows` e alterações no nosso `installer.iss` (`IsUpdating`, `GetInstallDir`, `GetAppMutex`, `versions.txt`) e no
  `release.yml`. Feito com `windows-sys` (não o crate `windows`).
- **Telemetria** (`metrics_id`/`system_id`): proibida pelo nosso ADR.
- **Cliente HTTP e executor**: o Zed usa `http_client`/`smol` do GPUI; o ADR pede `ureq` em thread dedicada. A lógica de
  estados é a mesma, a forma muda.
- **Nightly por SHA**: só faz sentido se houver canal nightly. Nosso `prerelease` usa semver (fora da v1).

## Fontes

Código (Zed fixado no commit `6501df15765d66d6c2a2b66b1eb22434ccef3f86`; os demais, branch `main` lida em 2026-11-23, sem SHA fixado):

- https://github.com/zed-industries/zed/blob/6501df15765d66d6c2a2b66b1eb22434ccef3f86/crates/auto_update/src/auto_update.rs
- https://github.com/zed-industries/zed/blob/6501df15765d66d6c2a2b66b1eb22434ccef3f86/crates/auto_update_ui/src/auto_update_ui.rs
- https://github.com/zed-industries/zed/blob/6501df15765d66d6c2a2b66b1eb22434ccef3f86/crates/release_channel/src/lib.rs
- https://github.com/zed-industries/zed/blob/6501df15765d66d6c2a2b66b1eb22434ccef3f86/docs/src/reference/all-settings.md (seção "Auto Update")
- https://github.com/zed-industries/zed/blob/6501df15765d66d6c2a2b66b1eb22434ccef3f86/docs/src/installation.md
- https://github.com/zed-industries/zed/blob/6501df15765d66d6c2a2b66b1eb22434ccef3f86/docs/src/development/linux.md (`ZED_UPDATE_EXPLANATION`)
- https://github.com/zed-industries/zed/blob/6501df15765d66d6c2a2b66b1eb22434ccef3f86/script/install.sh (host `cloud.zed.dev`, linha 86)
- https://github.com/microsoft/vscode/blob/main/src/vs/platform/update/electron-main/abstractUpdateService.ts
- https://github.com/microsoft/vscode/blob/main/src/vs/platform/update/electron-main/updateService.win32.ts
- https://github.com/microsoft/vscode/blob/main/src/vs/platform/update/electron-main/updateService.darwin.ts
- https://github.com/microsoft/vscode/blob/main/src/vs/platform/update/electron-main/updateService.linux.ts
- https://github.com/microsoft/vscode/blob/main/src/vs/platform/update/common/update.config.contribution.ts

Documentação e ferramentas:

- https://github.com/tauri-apps/tauri-docs/blob/v2/src/content/docs/plugin/updater.mdx
- https://sparkle-project.org/documentation/security-and-reliability/
- https://sparkle-project.org/documentation/publishing/
- https://github.com/vslavik/winsparkle (README)
- https://github.com/jaemk/self_update (README)
- https://github.com/axodotdev/cargo-dist/blob/main/book/src/installers/updater.md
- https://github.com/jedisct1/minisign (README)
- https://github.com/theupdateframework/specification/blob/master/tuf-spec.md

Lacunas desta pesquisa (não verificadas): valor de `updateUrl` no `product.json` do VS Code, funcionamento interno do
Squirrel.Mac e do `inno_updater`, nota de versão 1.11.0 do Sparkle, política de GPG do `dnf` para RPM local, e hosts de
redirecionamento atuais das Releases do GitHub. Já resolvidas depois da primeira versão deste relatório: o código do
`auto_update_helper` do Zed foi lido (seção "Zed em detalhe") e o host do endpoint do Zed é `cloud.zed.dev`
(`script/install.sh` linha 86). Context7 não estava disponível nesta sessão. Tudo foi lido do código e da documentação
oficial via download direto.
