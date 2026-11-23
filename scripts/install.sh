#!/usr/bin/env bash
# ==============================================================================
# dbmodeling - Assistente de Instalação, Atualização e Desinstalação Oficial
# Suporta: Linux (qualquer distro x86_64), Ubuntu/Debian, Fedora/RHEL e macOS.
#
# Linux, por padrão (SEM sudo): instala o tarball `db-modeling-linux-x86_64.tar.gz` em
# ~/.local/db-modeling.app, cria o symlink ~/.local/bin/db-modeling, o atalho .desktop, os
# ícones e o tipo MIME do usuário. É a instalação que se atualiza sozinha dentro do app.
# Linux `--system`: instala o .deb/.rpm com sudo (e as dependências gráficas), como antes;
# essa instalação NÃO se atualiza sozinha (atualize com apt/dnf).
#
# Integridade: o pacote é baixado pela tag resolvida da release (não por
# `releases/latest`) e o SHA-256 dele é conferido contra o `SHA256SUMS.txt`
# publicado na mesma release antes de qualquer instalação (e de qualquer `sudo`).
# Divergência aborta.
#
# Uso:
#   install.sh [--system] [arquivo-local]   instala (arquivo-local: tarball, .deb ou .rpm)
#   install.sh --uninstall [--system]       remove
#
# Variáveis opcionais:
#   DBML_REPO     repositório `dono/nome` (padrão: dbmodeling/dbml)
#   DBML_VERSION  tag a instalar, ex. v0.1.4 (padrão: a última release)
#   DBML_PREFIX   prefixo do modo de usuário (padrão: ~/.local)
# ==============================================================================
set -euo pipefail

REPO="${DBML_REPO:-dbmodeling/dbml}"
APP_NAME="db-modeling"
DISPLAY_NAME="dbmodeling"
SUMS_FILE="SHA256SUMS.txt"

BOLD="\033[1m"
GREEN="\033[32m"
YELLOW="\033[33m"
RED="\033[31m"
BLUE="\033[34m"
RESET="\033[0m"

# Calcula o SHA-256 de um arquivo (sha256sum no Linux, shasum no macOS).
sha256_of() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | awk '{print $1}'
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$1" | awk '{print $1}'
    else
        echo "Nenhuma ferramenta de SHA-256 encontrada (sha256sum/shasum)." >&2
        return 127
    fi
}

# Confere `arquivo` contra a linha de `nome` em `sums`. Retorna != 0 se divergir ou faltar.
#   verify_sha256 <arquivo> <nome-na-release> <SHA256SUMS.txt>
verify_sha256() {
    local file="$1" name="$2" sums="$3" expected actual
    expected="$(awk -v n="$name" '{f=$2; sub(/^\*/, "", f); if (f == n) {print $1; exit}}' "$sums")"
    if [ -z "$expected" ]; then
        echo "O ${SUMS_FILE} da release não lista ${name}." >&2
        return 1
    fi
    actual="$(sha256_of "$file")" || return 1
    if [ "$actual" != "$expected" ]; then
        echo "SHA-256 divergente para ${name}: esperado ${expected}, obtido ${actual}." >&2
        return 1
    fi
}

# Tag da release a instalar: DBML_VERSION ou a última publicada.
resolve_tag() {
    if [ -n "${DBML_VERSION:-}" ]; then
        echo "$DBML_VERSION"
        return 0
    fi
    local tag
    tag="$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
        | tr ',' '\n' \
        | sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' \
        | head -n 1)"
    case "$tag" in
        v[0-9]*.[0-9]*.[0-9]*)
            echo "$tag"
            ;;
        *)
            echo "Não foi possível descobrir a última release de ${REPO}." >&2
            return 1
            ;;
    esac
}

# Baixa `nome` da release `tag` para `destino` e verifica o SHA-256; apaga o arquivo se falhar.
#   download_verified <tag> <nome> <destino>
download_verified() {
    local tag="$1" name="$2" dest="$3" base sums
    base="https://github.com/${REPO}/releases/download/${tag}"
    sums="$(mktemp)"
    if ! curl -fsSL "${base}/${SUMS_FILE}" -o "$sums"; then
        rm -f "$sums"
        echo "Não foi possível baixar ${SUMS_FILE} da release ${tag}; instalação abortada." >&2
        return 1
    fi
    if ! curl -fsSL "${base}/${name}" -o "$dest"; then
        rm -f "$sums" "$dest"
        echo "Erro ao baixar ${name} da release ${tag}." >&2
        return 1
    fi
    if ! verify_sha256 "$dest" "$name" "$sums"; then
        rm -f "$sums" "$dest"
        echo "Verificação de integridade falhou; instalação abortada." >&2
        return 1
    fi
    rm -f "$sums"
    echo -e "${GREEN}✓ Integridade confirmada (SHA-256) para ${name} (${tag}).${RESET}"
}

APP_BUNDLE="${APP_NAME}.app"
LINUX_TARBALL="${APP_NAME}-linux-x86_64.tar.gz"
APP_MIME_FILE="${APP_NAME}.xml"
ICON_SVG_REL="scalable/apps/${APP_NAME}.svg"
ICON_PNG_REL="256x256/apps/${APP_NAME}.png"

# Prefixo e pasta de dados do modo de usuário (lidos na hora, para os testes trocarem o HOME).
user_prefix() { echo "${DBML_PREFIX:-${HOME:?HOME não definido}/.local}"; }
user_data_dir() { echo "${XDG_DATA_HOME:-${HOME:?HOME não definido}/.local/share}"; }

# Copia o .desktop do tarball trocando `Exec=` e `Icon=` por caminhos absolutos (o app pode estar
# fora do PATH). Sem `sed`: o caminho nunca é interpretado como padrão.
#   write_desktop_entry <desktop-origem> <desktop-destino> <pasta-do-app>
write_desktop_entry() {
    local src="$1" dest="$2" app="$3" exe icon line
    exe="${app}/bin/${APP_NAME}"
    icon="${app}/share/icons/hicolor/${ICON_PNG_REL}"
    case "$exe" in
        *[\"\\\`\$]*)
            echo "O caminho de instalação tem caracteres que um .desktop não aceita: ${exe}" >&2
            return 1
            ;;
    esac
    mkdir -p "$(dirname "$dest")"
    : > "$dest"
    while IFS= read -r line || [ -n "$line" ]; do
        case "$line" in
            Exec=*) printf 'Exec="%s" %%F\n' "$exe" >> "$dest" ;;
            Icon=*) printf 'Icon=%s\n' "$icon" >> "$dest" ;;
            *) printf '%s\n' "$line" >> "$dest" ;;
        esac
    done < "$src"
    chmod 0644 "$dest"
}

# Instala o tarball em ~/.local SEM sudo: pasta do app, symlink, .desktop, ícones e MIME.
#   install_user_bundle <tarball>
install_user_bundle() {
    local tarball="$1" prefix data app stage link rel
    prefix="$(user_prefix)"
    data="$(user_data_dir)"
    app="${prefix}/${APP_BUNDLE}"
    link="${prefix}/bin/${APP_NAME}"

    stage="$(mktemp -d)"
    if ! tar -xzf "$tarball" -C "$stage" --no-same-owner; then
        rm -rf "$stage"
        echo "Não foi possível extrair ${tarball}." >&2
        return 1
    fi
    if [ -n "$(find "$stage" -type l -print -quit)" ]; then
        rm -rf "$stage"
        echo "Tarball inválido: contém links simbólicos não permitidos." >&2
        return 1
    fi
    if [ ! -f "${stage}/${APP_BUNDLE}/bin/${APP_NAME}" ]; then
        rm -rf "$stage"
        echo "Tarball inválido: falta ${APP_BUNDLE}/bin/${APP_NAME}." >&2
        return 1
    fi

    mkdir -p "${prefix}/bin"
    # Troca a pasta inteira (a versão antiga some junto, como o `rsync --delete` do updater).
    rm -rf "${app}.new"
    mv "${stage}/${APP_BUNDLE}" "${app}.new"
    rm -rf "$stage" "$app"
    mv "${app}.new" "$app"

    if [ -e "$link" ] && [ ! -L "$link" ]; then
        echo -e "${YELLOW}${link} já existe e não é um symlink; mantido como está.${RESET}" >&2
    else
        ln -sfn "${app}/bin/${APP_NAME}" "$link"
    fi

    write_desktop_entry "${app}/share/applications/${APP_NAME}.desktop" \
        "${data}/applications/${APP_NAME}.desktop" "$app"
    for rel in "$ICON_SVG_REL" "$ICON_PNG_REL"; do
        install -D -m 0644 "${app}/share/icons/hicolor/${rel}" "${data}/icons/hicolor/${rel}"
    done
    install -D -m 0644 "${app}/share/mime/packages/${APP_MIME_FILE}" \
        "${data}/mime/packages/${APP_MIME_FILE}"

    # Caches do desktop: melhor esforço, nunca falham a instalação.
    refresh_desktop_caches "$data"
}

# Atualiza os caches do desktop do usuário: melhor esforço, nunca falha a operação.
refresh_desktop_caches() {
    local data="$1"
    if command -v update-desktop-database >/dev/null 2>&1; then
        update-desktop-database -q "${data}/applications" 2>/dev/null || true
    fi
    if command -v update-mime-database >/dev/null 2>&1; then
        update-mime-database "${data}/mime" 2>/dev/null || true
    fi
    if command -v gtk-update-icon-cache >/dev/null 2>&1; then
        gtk-update-icon-cache -q -t -f "${data}/icons/hicolor" 2>/dev/null || true
    fi
}

# Remove a instalação de usuário. Retorna 1 se não havia nenhuma.
uninstall_user_bundle() {
    local prefix data app link rel
    prefix="$(user_prefix)"
    data="$(user_data_dir)"
    app="${prefix}/${APP_BUNDLE}"
    link="${prefix}/bin/${APP_NAME}"
    [ -d "$app" ] || return 1
    rm -rf "$app"
    # só apaga o symlink se for o nosso
    if [ -L "$link" ] && [ "$(readlink "$link")" = "${app}/bin/${APP_NAME}" ]; then
        rm -f "$link"
    fi
    rm -f "${data}/applications/${APP_NAME}.desktop" "${data}/mime/packages/${APP_MIME_FILE}"
    for rel in "$ICON_SVG_REL" "$ICON_PNG_REL"; do
        rm -f "${data}/icons/hicolor/${rel}"
    done
    refresh_desktop_caches "$data"
    return 0
}

# Avisa (sem sudo) das bibliotecas do sistema que o binário instalado não encontra.
warn_missing_libs() {
    local app="$1" missing
    command -v ldd >/dev/null 2>&1 || return 0
    # `ldd` sai != 0 para binário estático/script: não é erro de instalação.
    missing="$(ldd "${app}/bin/${APP_NAME}" 2>/dev/null | awk '/not found/ {print $1}' | tr '\n' ' ')" || missing=""
    if [ -n "$missing" ]; then
        echo -e "${YELLOW}Bibliotecas do sistema ausentes: ${missing}${RESET}" >&2
        echo "Instale-as pelo gerenciador de pacotes, ou use --system (instala as dependências com sudo)." >&2
    fi
}

# Permite carregar só as funções (testes): DBML_INSTALL_LIB=1 source scripts/install.sh
if [ "${DBML_INSTALL_LIB:-}" = "1" ]; then
    return 0 2>/dev/null || exit 0
fi

echo -e "${BOLD}${BLUE}"
echo "=========================================================="
echo "          dbmodeling - Assistente de Instalação          "
echo "=========================================================="
echo -e "${RESET}"

usage() {
    cat <<EOF
Uso: install.sh [--system] [arquivo-local]
     install.sh --uninstall [--system]

  (padrão)     Linux: instala o tarball em ~/.local, sem sudo (atualiza sozinho pelo app).
  --system     Linux: instala o .deb/.rpm com sudo (atualize com apt/dnf).
  --uninstall  remove a instalação (a de ~/.local, ou a de sistema com --system).
  arquivo-local  tarball (.tar.gz), .deb ou .rpm já baixado; não é verificado por SHA-256.
EOF
}

MODE="user"
UNINSTALL=0
LOCAL_FILE=""
while [ $# -gt 0 ]; do
    case "$1" in
        --system) MODE="system" ;;
        --uninstall|-u) UNINSTALL=1 ;;
        -h|--help) usage; exit 0 ;;
        -*) echo "Opção desconhecida: $1" >&2; usage >&2; exit 2 ;;
        *) LOCAL_FILE="$1" ;;
    esac
    shift
done

# Instalação de usuário no Linux (padrão): tarball em ~/.local, sem sudo.
run_user_install() {
    local tarball="$LOCAL_FILE" tag prefix
    case "$ARCH" in
        x86_64|amd64) ;;
        *)
            echo -e "${RED}O tarball de usuário só existe para x86_64 (esta máquina: ${ARCH}).${RESET}" >&2
            exit 1
            ;;
    esac
    echo -e "${BOLD}Sistema detectado: Linux (${ARCH}); instalação de usuário, sem sudo${RESET}"
    if [ -z "$tarball" ] || [ ! -f "$tarball" ]; then
        tag="$(resolve_tag)" || exit 1
        echo "Baixando o tarball da release ${tag}..."
        tarball="${WORKDIR}/${LINUX_TARBALL}"
        download_verified "$tag" "$LINUX_TARBALL" "$tarball" || exit 1
    fi
    prefix="$(user_prefix)"
    echo "Instalando em ${prefix}/${APP_BUNDLE}..."
    install_user_bundle "$tarball" || exit 1
    echo -e "${GREEN}${BOLD}✓ ${DISPLAY_NAME} instalado/atualizado com sucesso em ${prefix}/${APP_BUNDLE}!${RESET}"
    warn_missing_libs "${prefix}/${APP_BUNDLE}"
    if ! command -v rsync >/dev/null 2>&1; then
        echo -e "${YELLOW}rsync não encontrado: o app precisa dele para se atualizar sozinho (ex.: sudo apt install rsync / sudo dnf install rsync).${RESET}" >&2
    fi
    case ":${PATH}:" in
        *":${prefix}/bin:"*) ;;
        *) echo -e "${YELLOW}Adicione ${prefix}/bin ao PATH para abrir com o comando db-modeling.${RESET}" ;;
    esac
    echo ""
    echo -e "${BOLD}Você já pode abrir o ${DISPLAY_NAME} pelo menu do seu sistema ou digitando:${RESET}"
    echo -e "  • App Desktop:      ${GREEN}${prefix}/bin/db-modeling${RESET}"
    exit 0
}

# Modo de Desinstalação
if [ "$UNINSTALL" = "1" ]; then
    echo -e "${YELLOW}==> Desinstalando ${DISPLAY_NAME}...${RESET}"
    if [ "$(uname -s)" = "Linux" ] && [ "$MODE" = "user" ] && uninstall_user_bundle; then
        echo -e "${GREEN}✓ ${DISPLAY_NAME} removido de $(user_prefix)/${APP_BUNDLE} (sem sudo).${RESET}"
        exit 0
    fi
    if [ "$(uname -s)" = "Darwin" ]; then
        if [ -d "/Applications/DB Modeling.app" ]; then
            sudo rm -rf "/Applications/DB Modeling.app"
            sudo rm -f "/usr/local/bin/dbml" "/usr/local/bin/db-modeling"
            echo -e "${GREEN}✓ ${DISPLAY_NAME} e CLI (dbml) desinstalados com sucesso do macOS.${RESET}"
        else
            echo "O aplicativo não foi encontrado em /Applications."
        fi
    elif command -v apt-get &>/dev/null; then
        sudo apt-get remove -y "${APP_NAME}"
        echo -e "${GREEN}✓ Pacote ${APP_NAME} removido com sucesso via APT.${RESET}"
    elif command -v dnf &>/dev/null; then
        sudo dnf remove -y "${APP_NAME}"
        echo -e "${GREEN}✓ Pacote ${APP_NAME} removido com sucesso via DNF.${RESET}"
    fi
    exit 0
fi

# Detecção do Sistema Operacional
OS="$(uname -s)"
ARCH="$(uname -m)"

# Diretório temporário privado (evita /tmp previsível) removido ao sair.
WORKDIR="$(mktemp -d)"
chmod 755 "$WORKDIR" # o `_apt` precisa ler o .deb baixado
trap 'rm -rf "$WORKDIR"' EXIT

if [ "$OS" = "Linux" ] && [ "$MODE" = "user" ]; then
    run_user_install
fi

case "$OS" in
    Darwin)
        echo -e "${BOLD}Sistema detectado: macOS (${ARCH})${RESET}"
        TAG="$(resolve_tag)" || exit 1
        echo "Baixando o Assistente de Instalação Apple Package (.pkg) da release ${TAG}..."
        TMP_PKG="${WORKDIR}/db-modeling-setup.pkg"
        download_verified "$TAG" "db-modeling-macos-setup.pkg" "$TMP_PKG" || exit 1
        echo "Instalando em /Applications..."
        sudo installer -pkg "$TMP_PKG" -target /
        echo -e "${GREEN}${BOLD}✓ ${DISPLAY_NAME} instalado com sucesso em /Applications!${RESET}"
        exit 0
        ;;
    Linux)
        DISTRO="unknown"
        if [ -f /etc/os-release ]; then
            . /etc/os-release
            DISTRO="${ID:-unknown}"
        fi
        echo -e "${BOLD}Sistema detectado: Linux (${DISTRO} ${ARCH})${RESET}"

        case "$DISTRO" in
            ubuntu|debian|pop|linuxmint|elementary)
                echo -e "${YELLOW}==> Verificando dependências gráficas nativas no Ubuntu/Debian...${RESET}"
                DEPS=(libfontconfig1 libfreetype6 libasound2 libxkbcommon0 libxkbcommon-x11-0 libvulkan1 libxcb1 hicolor-icon-theme)
                MISSING=()
                for dep in "${DEPS[@]}"; do
                    if ! dpkg -s "$dep" &>/dev/null; then
                        MISSING+=("$dep")
                    fi
                done

                if [ ${#MISSING[@]} -gt 0 ]; then
                    echo -e "${BLUE}Instalando dependências ausentes: ${MISSING[*]}${RESET}"
                    sudo apt-get update -y
                    sudo apt-get install -y "${MISSING[@]}"
                else
                    echo -e "${GREEN}✓ Todas as dependências do sistema já estão satisfeitas.${RESET}"
                fi

                # Arquivo local .deb fornecido como argumento (confiança do usuário) ou release verificada
                DEB_FILE="$LOCAL_FILE"
                if [ -z "$DEB_FILE" ] || [ ! -f "$DEB_FILE" ]; then
                    TAG="$(resolve_tag)" || exit 1
                    echo "Baixando o pacote .deb da release ${TAG}..."
                    DEB_FILE="${WORKDIR}/db-modeling-latest.deb"
                    download_verified "$TAG" "db-modeling-linux-amd64.deb" "$DEB_FILE" || exit 1
                fi

                echo "Instalando ou atualizando pacote..."
                sudo apt-get install -y "$DEB_FILE"
                echo -e "${GREEN}${BOLD}✓ ${DISPLAY_NAME} instalado/atualizado com sucesso!${RESET}"
                ;;

            fedora|rhel|centos|rocky|alma)
                echo -e "${YELLOW}==> Instalando via DNF com resolução automática de dependências...${RESET}"
                RPM_FILE="$LOCAL_FILE"
                if [ -z "$RPM_FILE" ] || [ ! -f "$RPM_FILE" ]; then
                    TAG="$(resolve_tag)" || exit 1
                    echo "Baixando o pacote .rpm da release ${TAG}..."
                    RPM_FILE="${WORKDIR}/db-modeling-latest.rpm"
                    download_verified "$TAG" "db-modeling-linux-x86_64.rpm" "$RPM_FILE" || exit 1
                fi

                sudo dnf install -y "$RPM_FILE"
                echo -e "${GREEN}${BOLD}✓ ${DISPLAY_NAME} instalado/atualizado com sucesso no Fedora!${RESET}"
                ;;

            *)
                echo -e "${YELLOW}Distribuição não reconhecida diretamente (${DISTRO}).${RESET}"
                echo "Por favor instale manualmente o pacote .deb ou .rpm adequado."
                exit 1
                ;;
        esac
        ;;
    *)
        echo -e "${RED}Sistema operacional não suportado por este script: ${OS}${RESET}"
        echo "No Windows, por favor execute o 'db-modeling-windows-setup.exe'."
        exit 1
        ;;
esac

echo ""
echo -e "${BOLD}Você já pode abrir o ${DISPLAY_NAME} pelo menu do seu sistema ou digitando:${RESET}"
echo -e "  • App Desktop:      ${GREEN}db-modeling${RESET}"
echo -e "  • CLI Web Server:   ${GREEN}dbml run schema.dbml${RESET}"
