# Auto-update do App Desktop

O App Desktop se atualiza sozinho, no modelo do Zed: checa ao abrir e a cada hora, baixa, verifica, instala os
arquivos na hora e mostra **Reiniciar para atualizar** na title bar. Só o desktop: a CLI Web e a extensão do VS Code
ficam de fora (a extensão atualiza pelo Marketplace). É uma exceção consciente à sincronia das três modalidades,
registrada no [ADR 0010](../openspec/adr/0010-auto-update-desktop.md).

Este documento descreve o comportamento **que existe no código** da branch `feat/auto-update-desktop`. O que ainda
não existe ou não foi verificado está marcado como lacuna, "não verificado" ou "pendente". A parte de interface
(textos, ações, paleta) está em [05-ux.md](05-ux.md#atualização-do-app-desktop). A pesquisa que embasou o desenho,
com o código do Zed fixado em um commit, está em [pesquisa/auto-update-desktop.md](pesquisa/auto-update-desktop.md).

Nesta página:

- [Como funciona](#como-funciona)
- [Configuração](#configuração)
- [Privacidade](#privacidade)
- [Segurança](#segurança)
- [Procedimento de retirada de release](#procedimento-de-retirada-de-release)
- [Rotação e backup das chaves](#rotação-e-backup-das-chaves)
- [Política de compatibilidade](#política-de-compatibilidade)
- [Se o relançamento falhar](#se-o-relançamento-falhar)
- [Notas de release da 0.4.0](#notas-de-release-da-040)
- [Limitações conhecidas](#limitações-conhecidas)
- [Arquivos e crates](#arquivos-e-crates)
- [Dependências novas](#dependências-novas)
- [Testes manuais pendentes](#testes-manuais-pendentes)
- [Lacunas entre o spec e o código](#lacunas-entre-o-spec-e-o-código)

## Como funciona

### Ciclo

1. **Quando.** Uma thread própria (`dbm-updater`) faz a primeira checagem **assim que o app abre** e depois uma a
   cada 1 h, contada do início da última tentativa (sucesso ou falha: não há retentativa antecipada). A checagem
   manual tem prioridade. Nada disso roda na thread da interface nem no frame loop. Se o sistema foi suspenso (o
   relógio de parede avança mais de 30 s além do monotônico), só as fases de rede recomeçam, nunca a instalação.
2. **Uma rodada**, nesta ordem: política (`auto_update` e razões de desativação) → plataforma suportada → **trava
   entre instâncias** → limpeza de órfãos com mais de 24 h → manifesto → assinatura → regra de versão →
   artefato da plataforma → download → SHA-256. Se qualquer passo falha, nada é instalado.
3. **Instalar na hora.** Depois de baixar e verificar, o instalador da plataforma roda imediatamente, sem esperar o
   reinício. Antes de montar, extrair ou executar o artefato, o SHA-256 é recalculado.
4. **Reiniciar.** A versão nova só roda depois de reiniciar (**Reiniciar para atualizar**); no Windows, sair do app
   também conclui a troca.
5. **Aviso pós-reinício.** O processo que instalou grava `update-notice.txt` na pasta de configuração com a versão e
   o link das notas; o processo novo lê, apaga e mostra "Atualizado para X". O aviso só vale se a versão em execução
   for a gravada.

A atualização só é instalada se a versão do manifesto for **maior** (semver) que a em execução e que a versão que este
processo já instalou e aguarda reinício. Não há rebaixamento.

### Máquina de estados

`UpdateStatus` (crate `updater`): não existe "baixada mas não instalada".

| Estado | Significado | Sai por |
|---|---|---|
| `Idle` | repouso | `begin_check` |
| `Checking` | consultando o manifesto | `found_update`, `no_update` ou `fail` |
| `Downloading(v)` | baixando e verificando | `download_verified` ou `fail` |
| `Installing(v)` | instalando os arquivos | `installed` ou `fail` |
| `Updated(v)` | instalada, falta reiniciar | nova checagem |
| `UpToDate` | checagem **manual** sem novidade (visível por 4 s) | `dismiss` |
| `Errored` | falha em operação **manual** | `dismiss` ou nova checagem |

A checagem automática que falha volta a repouso em silêncio; a manual vira `Errored`. Falha na fase `Installing`
**invalida** a pendência anterior (os arquivos podem ser uma mistura de versões, então a UI não oferece reiniciar);
falha em `Checking` ou `Downloading` não toca no disco e a mantém.

### Por plataforma

| | Windows | macOS (Apple Silicon) | Linux (x86_64) |
|---|---|---|---|
| Artefato | `db-modeling-windows-setup.exe` | `db-modeling-macos-arm64.dmg` | `db-modeling-linux-x86_64.tar.gz` |
| Onde atualiza | a pasta do app (`{app}`), instalação **por usuário** | o bundle em execução (`DB Modeling.app`) | `<prefixo>/db-modeling.app` |
| Como instala | instalador Inno com `/verysilent /update=true /nocloseapplications /norestart`, que grava em `{app}\install` e cria `{app}\updates\versions.txt` | `hdiutil attach -nobrowse -readonly -plist -mountroot <tmp privado>`, `rsync -av --delete --exclude "Icon?"` do `.app` montado sobre o bundle, `hdiutil detach -force` sempre | `tar -xzf ... --no-same-owner` em diretório privado e `rsync -av --delete db-modeling.app <prefixo>/` |
| Quem faz a troca final | o **helper** (`tools\auto_update_helper.exe`) | o próprio `rsync` | o próprio `rsync` |
| Verificação extra | Authenticode do instalador (SignPath, pelo SO) | `codesign --verify --deep --strict` e Team ID do manifesto, **só** se o manifesto traz `team_id` (build assinado) | confere que o tarball traz `db-modeling.app/bin/db-modeling` antes de tocar no prefixo |
| Reinício | `auto_update_helper.exe --launch true --wait-pid <pid>` e o app sai | processo curto `db-modeling --relaunch-after <pid>`, depois `cx.quit()` | igual ao macOS |
| Ao sair sem reiniciar | o app roda `auto_update_helper.exe --launch false` e espera | nada (os arquivos já foram trocados) | nada |
| Nunca | UAC nem `/ALLUSERS` | copiar antes de a assinatura passar (quando há `team_id`) | usar root, `sudo`, `pkexec`, `apt` ou `dnf` |

**Windows em detalhe.** O helper cobre `db-modeling.exe`, `dbml.exe`, `icon.ico` e o próprio helper. Antes de trocar,
espera o PID do app (até 30 s; estourar o prazo não é erro) e pede ao Restart Manager que libere `db-modeling.exe`.
Cada job (mover o antigo para `old\`, mover o novo de `install\`, apagar `updates`, `install` e `old`) tem prazo de 2 s
e repete a cada 50 ms em erro de E/S. Se um job falha, desfaz os anteriores em ordem inversa e **não relança**. Se
`dbml.exe` está em uso (`dbml run` aberto), é uma falha limpa com mensagem própria e o app anterior fica intacto. O
helper mostra a janela "Atualizando…" e, em falha, uma caixa de erro em pt-BR ou en (pelo idioma do Windows), e anota
tudo em `tools\auto_update_helper.log`. O instalador silencioso tem prazo de 10 minutos; passado disso o app o encerra
e a atualização falha.

**Instalação pendente na abertura (Windows).** Se o app foi encerrado sem passar pelo helper (queda de energia,
`kill`), o próximo início confere o disco: flag e `install\` presentes viram "Reiniciar para atualizar"; troca
interrompida (`old\` presente) é revertida a partir de `old\`; restos sem flag são apagados.

**Quando a atualização fica indisponível por layout (Linux).** O Linux só atualiza se o executável em uso estiver em
`<prefixo>/db-modeling.app/bin/db-modeling` e o prefixo não for de sistema (`/usr`, `/bin`, `/sbin`, `/lib`, `/lib64`,
`/etc`, `/var`, `/boot`, `/snap`). Fora disso (build de desenvolvimento, `/usr/bin` de pacote) o app não baixa nem
copia nada, e a UI diz o motivo: "Só a instalação pelo install.sh se atualiza sozinha; atualize manualmente" (fora do
layout) ou "Instalado numa pasta do sistema; atualize pelo gerenciador de pacotes ou pelo install.sh" (layout num
prefixo de sistema). Precisa de `tar` e `rsync` no `PATH`; sem eles a falha vira "Falha ao atualizar" na checagem manual. O
`.desktop`, os ícones e o symlink `~/.local/bin/db-modeling` são do `install.sh`; o updater só troca a pasta
`db-modeling.app`.

**Trava entre instâncias.** O app aceita várias janelas e processos, então o updater toma uma trava de arquivo
exclusiva em `<cache>/db-modeling/updates/lock` (`%LOCALAPPDATA%`, `~/Library/Caches` ou `$XDG_CACHE_HOME`/`~/.cache`).
Quem não a obtém pula a rodada e, se foi uma checagem manual, mostra "Outra janela já está verificando atualizações".
Essa pasta `updates/` do cache é a única que a limpeza de órfãos toca; ela **não** é a pasta `updates\` do Windows em
`{app}`, que é do helper.

## Configuração

### `auto_update`

Preferência booleana em `settings.txt`, padrão **ligada**: `auto_update=false` desliga a checagem automática (nenhuma
requisição). A checagem manual continua funcionando. É um interruptor nas configurações (**Atualizar automaticamente**),
uma ação na paleta (`ToggleAutoUpdate`) e vale na hora, sem reiniciar. Só o valor `false` desliga: qualquer outro texto
mantém o padrão. Local do arquivo: `%APPDATA%\db-modeling\settings.txt`, ou `$XDG_CONFIG_HOME/db-modeling/settings.txt`,
ou `~/.config/db-modeling/settings.txt` (também no macOS).

### Razões de desativação e precedência

Uma razão de desativação vem do **ambiente**, não da preferência. Enquanto existir, não há checagem nenhuma (nem
manual) e a UI mostra o texto no lugar da ação, com o link para as Releases. Ordem de precedência
(`updater::policy::disabled_reason`; a primeira que valer ganha):

1. Variável **`DBM_UPDATE_EXPLANATION`** (não vazia): override do usuário ou do empacotador; o valor é o texto mostrado.
2. Arquivo **`/usr/lib/db-modeling/update-explanation`** (não vazio), instalado pelos `.deb`/`.rpm`; um app aberto pelo
   `.desktop` não herda variável de ambiente de pacote, por isso existe o arquivo. O texto atual é "Instalado pelo
   gerenciador de pacotes; atualize com apt/dnf" e aparece sem tradução.
3. Sistema operacional ou arquitetura sem política conhecida.
4. Linux: nenhuma razão de ambiente (a decisão é o layout do tarball, descrito acima).
5. Windows ou macOS com o local do app indeterminável (macOS: binário fora de um `.app`). A política **falha fechada**.
6. **Windows** com o diretório do app sob `%ProgramFiles%` (`ProgramFiles`, `ProgramFiles(x86)` ou `ProgramW6432`).
7. **macOS** com o bundle sob App Translocation (`/AppTranslocation/` no caminho).
8. **Sem escrita** onde o app seria atualizado: no Windows, a pasta do app; no macOS, a pasta **pai** do bundle (nunca
   dentro do `.app`, para não quebrar o selo do `codesign`). É uma aproximação por arquivo de sonda: ACLs, volumes
   somente leitura e cotas podem divergir do resultado real.

Combinação de `auto_update` e razão: automática só com `auto_update` ligado **e** nenhuma razão; manual só com nenhuma
razão.

### Gancho de teste: `DBM_UPDATE_URL`

`DBM_UPDATE_URL` (origem HTTP local que substitui `https://github.com/...`) e `DBM_UPDATE_TEST_PUBLIC_KEY` (chave
pública de teste no lugar das embutidas) **só existem em `debug_assertions` ou com a feature `harness`**. Em release o
código nem é compilado. O teste `crates/updater-net/tests/release_has_no_hook.rs` (rodar com
`cargo test -p updater-net --release`) varre o binário atrás dos nomes; o `release.yml` não pode habilitar `harness`
(conferir com `grep -n harness .github/workflows/release.yml`). Nunca documente o gancho como recurso de usuário.

## Privacidade

- **O que sai.** Requisições HTTPS ao GitHub: o manifesto (`.../releases/latest/download/latest.json`), a assinatura
  (`latest.json.sig`) e, havendo versão nova, o artefato. O cabeçalho identificador é
  **`User-Agent: db-modeling-updater/<versão do app>`**. Quando se aplicam, vão também `If-None-Match` (o `ETag` do
  manifesto) e `Range` (retomar um download interrompido).
- **O que não sai.** O código **não envia** SO nem arquitetura (o manifesto traz todas as plataformas e o app escolhe a
  sua localmente), identificador de máquina ou de usuário, telemetria, caminhos, nomes de arquivo ou qualquer
  conteúdo de esquema. O `metrics_id`/`system_id` do Zed não foi copiado.
- **O que o servidor vê de qualquer forma.** O endereço IP, o nome do host (SNI do TLS) e o que o GitHub registra em
  suas próprias Releases (contagem de downloads, por exemplo). Os redirecionamentos do GitHub podem levar a hosts
  `*.githubusercontent.com`, que também veem o IP.
- **Proxy e certificados.** Honra `HTTPS_PROXY` (ou `https_proxy`) e usa as raízes de certificado do sistema
  (`rustls-native-certs`). Não lê `HTTP_PROXY`, `ALL_PROXY` nem `NO_PROXY`. Proxy com autenticação: não verificado.
- **Primeira checagem sem consentimento.** Com a preferência padrão (ligada), a primeira consulta sai ao abrir o app,
  como no Zed. Quem quer evitar isso precisa desligar antes da primeira abertura (edite `settings.txt` com
  `auto_update=false`) ou definir `DBM_UPDATE_EXPLANATION`.
- **Como desligar.** (a) Configurações → **Atualizar automaticamente** (e a ação na paleta); (b) `auto_update=false`
  em `settings.txt`; (c) `DBM_UPDATE_EXPLANATION` com um texto qualquer, que desliga também a checagem manual.
  Desligar (a) ou (b) **não** impede a checagem manual, que consulta o servidor quando o usuário pede.

## Segurança

O Zed só confia em TLS; o nosso download vem de uma Release do GitHub, então o desenho acrescenta verificação própria.

- **Manifesto assinado com minisign (Ed25519).** O app só confia em `latest.json` cuja assinatura (`latest.json.sig`, o
  arquivo de assinatura completo de 4 linhas) for válida para **uma de duas chaves públicas embutidas**: a primária e
  a reserva (privada só offline), em `crates/updater/src/constants.rs`. A assinatura é verificada sobre os **bytes
  exatos** baixados, **antes** de interpretar o JSON.
- **SHA-256 do artefato.** Conferido ao baixar e **recalculado imediatamente antes** de montar, extrair ou executar.
  Divergência apaga o arquivo e a rodada falha.
- **macOS.** O download por `ureq` não recebe quarentena, então o Gatekeeper não confere nada: a nossa verificação é
  o único portão. Com `team_id` no manifesto (build assinado), `codesign --verify --deep --strict` e o Team ID do `.app`
  montado precisam bater **antes de qualquer cópia**; depois da cópia o bundle é conferido de novo. Sem `team_id`
  (build sem assinatura) a verificação é pulada.
- **Rebaixamento recusado.** Só instala semver estritamente maior que a versão em execução e que a já instalada
  aguardando reinício.
- **URLs.** Só HTTPS. Hosts permitidos: `github.com` e `*.githubusercontent.com`. Redirecionamentos são seguidos à
  mão (no máximo 5) e a política é aplicada a **cada salto**. O `url` de cada ativo e o `notes_url` do manifesto
  precisam começar por `https://github.com/dbmodeling/dbml/releases/` (`RELEASES_URL_PREFIX`, o mesmo prefixo que o
  aviso pós-reinício aceita) e não podem ter `%`, `\`, `@`, `..` nem `//` no restante. Um `notes_url` fora das
  Releases invalida o manifesto inteiro.
- **Limites.** Manifesto até 64 KiB, assinatura até 8 KiB, artefato até 512 MiB, espaço livre mínimo de 1 GiB.
  Timeouts: conexão e DNS 10 s, leitura e escrita 30 s, requisição pequena 30 s no total.
- **Diretório de downloads privado.** `0700` em Unix (arquivos `0600`); no Windows, uma DACL só do dono e do SYSTEM
  (**não verificado em Windows**). O `.part` nunca é usado como pronto.
- **Esquema desconhecido do manifesto** é ignorado como "sem atualização".
- **Binários do Windows.** O helper e o instalador são assinados pelo SignPath no `release.yml`; o helper é assinado
  **antes** de ser embutido no instalador.
- **Não coberto:** replay ou freeze do manifesto. Um manifesto antigo e válido continua aceito (não há `expires`) e só
  é barrado pela regra de versão. Fica para depois da v1.

## Procedimento de retirada de release

Sem rebaixamento, **não existe "desinstalar" uma versão ruim nos clientes**: quem já a instalou só sai dela com uma
versão **maior**. O job `publish` do `release.yml` gera e assina `latest.json`, sobe tudo como Release **rascunho**,
confere o rascunho com `scripts/verify-update-manifest.sh` e só então publica; esse job **ainda não rodou no Actions**
(o ensaio é do ticket 10), então os passos abaixo são o desenho, não um procedimento ensaiado.

1. **Conter (parar novas instalações da versão ruim `X`).** O app lê `releases/latest/download/latest.json`, ou seja,
   o manifesto da Release "latest" (a mais recente que não é rascunho nem pré-release). Duas formas, que podem ser
   combinadas:
   - marcar a Release `X` como **pré-release** na página do GitHub: pelo comportamento do GitHub, `latest` volta a
     apontar para a Release anterior (a ser confirmado no ensaio do ticket 10);
   - **regenerar o manifesto** da Release `X` apontando para a versão anterior `X-1` e assiná-lo de novo (só o CI tem
     a chave primária; precisa de um job de regeneração manual, **a criar**). Quem está em `X-1` não vê novidade; quem
     já está em `X` também não, porque não há rebaixamento.
2. **Corrigir.** Publicar uma versão **corretiva** com semver maior (`X+1`). É o único caminho que alcança quem
   instalou `X`. Antes, ensaiar com Release rascunho (o rascunho só vira público no último passo, para `latest.json` não
   apontar para ativo que não existe).
3. **Avisar.** Editar as notas da Release `X` (e abrir uma issue fixada) dizendo o que está errado e que `X+1` corrige.
4. **Se a falha impede o próprio updater de rodar** na versão `X`, o usuário precisa instalar `X+1` à mão (instalador
   da página de Releases ou `scripts/install.sh`), como na 0.3.x → 0.4.0.
5. **Não** apagar a Release `X`: quem a instalou precisa achar as notas e o aviso; prefira marcá-la como pré-release e
   deixar o aviso nas notas.

## Rotação e backup das chaves

**Chaves.** Dois pares minisign. A **primária** assina os manifestos no CI; sua privada fica no segredo
**`UPDATER_SIGNING_KEY`** do GitHub Actions. A **reserva** nunca vai ao CI: sua privada existe **só offline**. As duas
públicas estão embutidas no app e basta uma validar a assinatura.

**Regras (válidas sempre).**

- Nunca colocar chave privada no repositório, em issue, em log, em `.scratch/` ou em linha de comando (use entrada
  padrão ou arquivo de modo restrito).
- **Backup offline** das duas privadas, em mídia criptografada e em pelo menos dois locais. **Pendência do mantenedor**
  (ticket 02): copiar as duas privadas para o backup e conferir que a reserva saiu do disco de trabalho. Até concluir,
  a perda é um risco aberto.
- Preferir chaves protegidas por senha, com a senha guardada à parte.

**Se perder ou vazar a chave primária** (a reserva está intacta). Proposta, ainda não ensaiada:

1. Gerar um novo par.
2. Assinar **à mão, offline, com a reserva** o manifesto de uma versão nova cujo app embute `[reserva, nova]` em
   `PUBLIC_KEYS`. Os clientes atuais conhecem `[primária, reserva]`, então aceitam essa assinatura e passam a conhecer
   a nova chave.
3. Trocar o segredo `UPDATER_SIGNING_KEY` pela nova privada e revogar o antigo.
4. Quando a maioria já migrou, gerar uma reserva nova e embuti-la em outra versão (`[nova, reserva nova]`), assinada
   pela nova primária.

**Se perder a reserva** (a primária está intacta): gerar uma nova reserva e embuti-la numa versão comum, assinada pela
primária (`[primária, reserva nova]`); guardar a privada nova offline.

**Se perder as duas:** nenhum cliente já instalado consegue validar um manifesto novo. O updater das versões instaladas
fica inutilizado e todos precisam instalar à mão uma versão que embuta chaves novas. Por isso o backup é obrigatório.

Para trocar chaves é preciso mudar `PUBLIC_KEY_PRIMARY` e `PUBLIC_KEY_RESERVE` em `crates/updater/src/constants.rs`.

## Política de compatibilidade

O que o código faz hoje com os arquivos do usuário, em `session.rs` e `settings.rs`:

| Arquivo | Leitura | Escrita | Consequência entre versões |
|---|---|---|---|
| `session.json` | `serde` com `#[serde(default)]`, sem `deny_unknown_fields`: campo desconhecido é ignorado e campo ausente toma o padrão | regrava a estrutura inteira, com `version: 1` fixo | versão nova lê sessão antiga sem problema; versão antiga lê a sessão da nova ignorando o que não conhece, mas **ao salvar descarta os campos novos** |
| `settings.txt` (`chave=valor`) | chave desconhecida é ignorada; valor fora da faixa volta ao padrão | regrava só as chaves que conhece | mesma lógica: quem volta à versão antiga perde as chaves novas na primeira gravação (por exemplo, `auto_update` volta ao padrão ligado ao reabrir a versão nova) |
| `<arquivo>.dbml.layout.json` | — | as gravações de posições e arestas preservam as outras chaves (há testes) | seguro entre versões |
| `update-notice.txt` | só vale se a versão gravada for a em execução; é apagado ao ler | gravado só depois de instalar com sucesso | um aviso antigo nunca reaparece |

Arquivo ilegível (JSON inválido, erro de leitura) é **copiado para `<nome>.bak`**, os padrões são usados e o app avisa
na janela; nada é apagado calado.

Política adotada, a partir do comportamento acima:

- **Para frente (atualizar):** suportado. A versão nova precisa ler o que a antiga gravou. Campo novo em
  `session.json` ou `settings.txt` deve ser **opcional, com padrão**; renomear ou mudar o tipo de um campo existente
  exige leitura do formato antigo.
- **Para trás (voltar à versão antiga):** sem garantia formal, mas tolerante na prática: nada trava, e o pior caso é
  perder preferências ou campos novos da sessão. O updater nunca rebaixa; voltar exige instalar o instalador antigo à
  mão. Quem faz isso deve guardar uma cópia da pasta de configuração antes.
- **Lacunas** (não definidas no código): o campo `version` da sessão é sempre gravado como `1` e **nunca é lido**, então
  não há migração por versão; não há testes de compatibilidade entre versões do app; a regra "campo novo é sempre
  opcional" não é imposta por teste; o formato da pasta `<config>/recovery/` entre versões não foi verificado.

## Se o relançamento falhar

A versão nova **já está instalada** quando o relançamento falha. Abra o app de novo pelo menu de aplicativos, Dock ou
Menu Iniciar.

- **Windows.** Se o helper falha, ele desfaz tudo e o app anterior segue utilizável; a caixa de erro diz "A atualização
  falhou e a versão anterior foi restaurada. Abra o DB Modeling novamente." Se `dbml run` estava aberto, feche-o e
  reinicie o app. Se a mensagem disser que não foi possível restaurar tudo, **reinstale** com o instalador mais recente.
  O log fica em `{app}\tools\auto_update_helper.log`. Sem `tools\auto_update_helper.exe` (build de desenvolvimento), o
  app mostra "Falha ao reiniciar" e segue aberto.
- **macOS e Linux.** Se o processo curto de relançamento nem inicia, o app segue aberto e mostra "Falha ao reiniciar.
  Abra o app novamente". Se o app saiu e o processo curto não abriu o novo (ou o app antigo não saiu em 30 s), nada
  mais é aberto: o aviso "A versão nova já está instalada. Abra o DB Modeling novamente pelo menu de aplicativos." vai
  para `stderr` e para o log. No Linux do tarball também serve `~/.local/bin/db-modeling`.
- **Se o app não abre depois de uma atualização interrompida** (macOS: `rsync` morto no meio; Windows: falha parcial):
  reinstale pelo `.dmg`/`.pkg` (macOS), pelo instalador (Windows) ou por `scripts/install.sh` ou pelo tarball (Linux).
  Suas pastas de configuração e sessão não são tocadas.

## Notas de release da 0.4.0

Texto para colar nas notas da Release `v0.4.0`:

> **Primeira versão com atualização automática.** A partir da 0.4.0, o App Desktop procura versões novas ao abrir e a
> cada hora, baixa e instala sozinho, e mostra **Reiniciar para atualizar** na barra de título. Quem está na **0.3.x
> precisa instalar à mão uma vez** (instalador da Release ou `scripts/install.sh`): a 0.3.x não sabe se atualizar. O
> app não envia telemetria: só o número de versão, no `User-Agent`, vai ao GitHub. Para desligar: Configurações → **Atualizar
> automaticamente**. No Linux, a instalação de usuário (`~/.local`, pelo `install.sh` sem `sudo`) se atualiza sozinha;
> instalações por `.deb`/`.rpm` continuam sendo atualizadas por `apt`/`dnf`. No macOS, só Apple Silicon.

## Limitações conhecidas

- **macOS Intel fora**: a release só gera `arm64`; a plataforma `macos-x86_64` não existe no manifesto.
- **Windows ARM e Linux ARM fora.** Só `windows-x86_64`, `macos-aarch64` e `linux-x86_64`.
- **`.deb` e `.rpm` não se atualizam sozinhos**; só mostram a explicação. Use `apt` ou `dnf`.
- **Linux só atualiza a instalação em `<prefixo>/db-modeling.app`** (tarball), com `tar` e `rsync` instalados. Build
  de desenvolvimento e `/usr/bin` ficam "indisponíveis".
- **`rsync` interrompido no macOS** pode deixar o bundle inconsistente (e o selo do `codesign` inválido); não há
  rollback. É o mesmo risco do Zed, aceito no ADR; a saída é reinstalar.
- **Windows por máquina** (`%ProgramFiles%`) ou sem escrita: só a explicação, sem UAC.
- **A CLI `dbml` não vem no tarball nem nos pacotes Linux** (o `.deb`/`.rpm` também não a entregam). No Windows ela
  é trocada com o app; se estiver em uso (`dbml run`), a atualização falha de forma limpa.
- **Pré-release fora**: `releases/latest/download` não enxerga pré-lançamentos; só existe o canal `stable`, sem
  seletor.
- **Sem proteção contra replay/freeze** do manifesto (sem `expires`).
- **`ETag` só em memória**: a primeira consulta de cada sessão não é condicional.
- **Proxy:** só `HTTPS_PROXY`/`https_proxy`.
- **Windows e macOS não foram testados** (veja [Testes manuais pendentes](#testes-manuais-pendentes)).
- **Leitores de tela:** o GPUI fixado não tem região viva nem árvore de acessibilidade do sistema; o controle expõe
  `role` e rótulo, mas o anúncio das mudanças depende do leitor.

## Arquivos e crates

| Peça | Onde | O que faz |
|---|---|---|
| **`updater`** (crate pura) | `crates/updater/` | Manifesto, plataforma, política e razão de desativação, verificação (SHA-256 e minisign), máquina de estados, agendamento, trava e a trait `Transport`. Sem GPUI nem rede concreta; constantes em `constants.rs`. |
| **`updater-net`** (rede) | `crates/updater-net/` | `ureq` + `rustls`, política de URL, diretório privado, `ETag`, download com limites e a rodada completa (`run_round`). Gancho de teste só em debug/`harness`. |
| **`auto-update-helper`** (só Windows) | `crates/auto-update-helper/` | Troca de arquivos com jobs `apply`/`rollback`, recuperação de restos, janela "Atualizando…". Binário `auto_update_helper.exe`. |
| Controlador e UI | `crates/app/src/auto_update.rs`, `updater_ui.rs`, `shell/updates.rs`, `update_notice.rs`, `relaunch.rs`, `update_install.rs` | Thread do updater, visão da title bar, aviso pós-reinício, relançamento e o ponto de plug dos instaladores. |
| Instalador Windows | `crates/app/src/install_windows.rs` + `scripts/windows/installer.iss` | Roda o Inno em `/update=true`, reconcilia restos, chama o helper. |
| Instalador macOS | `crates/app/src/install_macos.rs` | `hdiutil`, `codesign`/Team ID e `rsync`. |
| Instalador Linux | `crates/app/src/install_linux.rs`, `scripts/build-linux-tarball.sh`, `scripts/install.sh` | `tar` + `rsync` no prefixo do usuário; o `install.sh` instala o tarball por padrão (sem `sudo`) e `--system` instala `.deb`/`.rpm`. |
| Explicação dos pacotes | `scripts/linux/update-explanation` (instalado em `/usr/lib/db-modeling/update-explanation` pelo `build-deb.sh` e `build-fedora.sh`) | Desliga o auto-update nas instalações de pacote. |
| Preferência | `crates/app/src/settings.rs` (`auto_update`) | Interruptor persistido. |
| Release | `.github/workflows/release.yml`, `ci.yml` | Gera o tarball, assina o helper e o instalador e, no job `publish`, gera, assina e confere `latest.json`/`latest.json.sig` numa Release rascunho antes de publicar (`scripts/{generate,sign,verify}-update-manifest.sh`). **Nunca rodou no Actions.** |

## Dependências novas

Versões exatas lidas do `Cargo.toml` da raiz (`=`), conferidas contra o `Cargo.lock`. Licenças lidas do `Cargo.toml`
de cada crate no registro local do Cargo.

| Crate | Versão | Usada em | Licença (SPDX) |
|---|---|---|---|
| `ureq` (`default-features = false`, feature `rustls`) | `=3.5.2` | `updater-net` | MIT OR Apache-2.0 |
| `rustls-native-certs` | `=0.8.5` | `updater-net` | Apache-2.0 OR ISC OR MIT |
| `url` | `=2.5.9` | `updater-net` | MIT OR Apache-2.0 |
| `fs4` (`default-features = false`, feature `sync`) | `=1.1.0` | `updater-net` (espaço livre em disco) | MIT OR Apache-2.0 |
| `minisign-verify` | `=0.3.0` | `updater` | MIT |
| `windows-sys` | `=0.61.2` | `updater-net` e `auto-update-helper`, só no Windows | MIT OR Apache-2.0 |
| `semver` | `=1.0.28` | `updater`, `updater-net` | MIT OR Apache-2.0 |
| `sha2` | `=0.11.1` | `updater` | MIT OR Apache-2.0 |
| `minisign` (só testes) | `=0.10.0` | `updater-net` (dev) | MIT |

O ticket citava o crate `windows`; a implementação usa **`windows-sys`**.

**Transitivas principais conferidas** (todas com licença na lista de `deny.toml`): `rustls` 0.23.46 (Apache-2.0 OR ISC
OR MIT), `rustls-webpki` 0.103.16 (ISC), `rustls-pki-types` 1.15.1 e `ureq-proto` 0.7.1 (MIT OR Apache-2.0),
`webpki-roots` 1.0.11 (CDLA-Permissive-2.0), `utf8-zero` 0.8.1 (MIT OR Apache-2.0); e, por `minisign` (testes),
`ct-codecs` (MIT), `scrypt`, `pbkdf2` (MIT OR Apache-2.0), `rpassword` e `rtoolbox` (Apache-2.0).

**`cargo deny`:** **não está instalado nesta máquina e não foi executado.** A conferência acima foi manual: toda licença
listada está entre as aceitas em `deny.toml` (`Apache-2.0`, `MIT`, `ISC`, `CDLA-Permissive-2.0`). Não foi verificado
`cargo deny check advisories`, `bans` nem `sources`. Rodar `cargo deny check` no CI (`security.yml`) é o veredito. O
`deny.toml` **não foi alterado** e a troca de licença do projeto para Apache-2.0 ainda não foi feita: o workspace
declara `PolyForm-Noncommercial-1.0.0` (as crates do workspace são ignoradas por `private = { ignore = true }`).

## Testes manuais pendentes

O código de `cfg(windows)` e de `cfg(target_os = "macos")` **nunca compilou nem rodou** no ambiente de desenvolvimento
(Fedora). **Windows e macOS ainda não foram testados** e tudo abaixo é "não verificado" até alguém rodar. No Linux, a
lógica e os instaladores têm testes automatizados (com `tar` e `rsync` reais), mas a matriz manual em Ubuntu e Fedora
também está pendente. O ticket 10 registra executor e data de cada item.

### Windows 10 e 11 (instalação por usuário)

1. `cargo test -p auto-update-helper` e `cargo clippy --workspace --all-targets -- -D warnings` no Windows (é a
   primeira compilação do `win.rs` e do `install_windows.rs`).
2. Instalar a versão N, publicar a N+1, esperar o app mostrar "Reiniciar para atualizar" e clicar: o app reabre na
   versão nova com a sessão restaurada.
3. Repetir só fechando o app (gancho `--launch false`): `{app}\updates`, `install` e `old` somem, ou sobra só o `old`
   com o helper antigo, que a próxima abertura limpa.
4. Com `dbml run` aberto: mensagem de `dbml.exe` em uso, rollback e app anterior intacto. Fechar o `dbml` e tentar de
   novo.
5. `kill` no `auto_update_helper` no meio e depois no Inno: a próxima abertura mostra "Reiniciar para atualizar" ou
   limpa os restos, sem perder o app.
6. Adicionar/Remover programas: `DisplayVersion` novo. `PATH` e associação `.dbml` intactos. O desinstalador remove
   tudo, inclusive `tools\`.
7. Instalação em `Program Files`: só a explicação, sem UAC.
8. Janela "Atualizando…" visível e legível, em pt-BR e en.
9. Conferir `tools\auto_update_helper.log`.
10. Helper ausente (build de dev): "Reiniciar" mostra "Falha ao reiniciar" e o app segue aberto.
11. Registrar o resultado do Restart Manager com o app GPUI (primeira vez que roda).
12. SignPath: o helper solto precisa ser aceito pela configuração de artefato; se não, criar
    `artifact-configuration-slug`.
13. ACL do diretório de download (`updater-net`, SDDL `D:P(A;OICI;FA;;;OW)(A;OICI;FA;;;SY)`): conferir com `icacls`.
14. Arquivo `dbml.exe` somente leitura: a checagem "em uso" dá falso positivo; confirmar o comportamento.

### macOS Apple Silicon

1. Build assinado em `/Applications`: atualizar, conferir `codesign --verify --deep --strict "/Applications/DB Modeling.app"`
   e `spctl -a -vv`, e ver se o Dock e a sessão voltam após o relançamento.
2. Build sem assinatura (manifesto sem `team_id`): atualiza sem chamar `codesign`.
3. Team ID forjado no manifesto: recusa, o app não muda e `hdiutil info` não lista o volume.
4. App rodando a partir do `.dmg` (App Translocation) ou com bundle sem escrita: só a explicação, sem download nem
   montagem.
5. `kill -9` no `rsync` no meio da cópia: ver se o app abre e se o `codesign` ainda passa (risco aceito, igual ao Zed).
6. Depois de qualquer falha, `hdiutil info` não mostra volume montado e não sobra `db-modeling-update-*` em `/tmp`.
7. Formato real da saída de `hdiutil attach -plist`, `codesign -dv` e `rsync`; a flag `-readonly` do `attach`; o spawn
   direto do executável no relançamento.
8. Compilar `cargo clippy -p app --all-targets -- -D warnings` no macOS (primeira vez que `install_macos.rs` roda com
   `cfg(macos)`).

### Linux (Ubuntu 22.04/24.04 e Fedora 40/41)

1. `./scripts/install.sh`: o ícone aparece no menu, `db-modeling` abre pelo atalho e o tipo MIME `.dbml` está associado.
2. Instalar a v1, publicar a v2 e conferir "Reiniciar para atualizar", o reinício e a notificação "Atualizado para X".
3. Instalar o `.deb`/`.rpm`: o app mostra a explicação e não faz tráfego de rede.
4. `--system` e depois `./scripts/install.sh --uninstall --system`.
5. App sem `rsync` instalado e com prefixo de outro dono: "Falha ao atualizar".
6. O binário do tarball roda nas duas famílias (bibliotecas de sistema).
7. O passo novo do `release.yml` (tarball) no runner real.
8. `.deb`: construir com `dpkg-deb` (não existe na máquina de desenvolvimento) e conferir
   `/usr/lib/db-modeling/update-explanation`.
9. Rodar `shellcheck` nos scripts (não estava instalado).

### Ensaio de rollout (ticket 10)

A primeira release com updater sai como Release **rascunho**; testar a cadeia 0.3.x → 0.4.0 → 0.4.1 numa máquina real
por plataforma antes de publicar.

## Lacunas entre o spec e o código

Conferidas na leitura do código em 2026-11-23. O spec é `.scratch/auto-update-desktop/spec.md`.

1. **Privacidade: SO e arquitetura não saem.** O spec diz que saem "versão, SO e arquitetura". O código envia só o
   `User-Agent: db-modeling-updater/<versão>` (SO e arquitetura nunca são enviados, pois o cliente escolhe a
   plataforma localmente). É mais restritivo; o texto do spec e da constante `USER_AGENT_PREFIX` deveriam refletir isso.
2. **Sem atalho de teclado próprio.** O spec (seção 3) diz "toda ação tem atalho e entrada na command palette".
   `CheckForUpdates`, `RestartToUpdate` e `ToggleAutoUpdate` têm entrada na paleta e (a primeira) no menu, mas
   `binding` vazio no catálogo. O princípio do projeto ("acessível por teclado") é atendido pela paleta e pelo foco,
   mas não há atalho direto.
3. **`deny.toml` e licença:** o ticket 09 prevê `cargo deny` limpo e a troca para Apache-2.0; `cargo deny` não roda
   aqui e a troca de licença não foi feita (veja [Dependências novas](#dependências-novas)).
4. **Compatibilidade de dados sem versão lida nem testes** (veja a [política](#política-de-compatibilidade)).
5. **Proteção contra freeze/replay (`expires`)** continua fora da v1, como o ADR já registra.
