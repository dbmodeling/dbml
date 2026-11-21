#!/usr/bin/env bash
# ==============================================================================
# dbmodeling - Assistente de Instalação, Atualização e Desinstalação Oficial
# Suporta: Ubuntu/Debian, Fedora/RHEL e macOS.
# Detecta e instala dependências do sistema automaticamente na hora.
#
# Integridade: o pacote é baixado pela tag resolvida da release (não por
# `releases/latest`) e o SHA-256 dele é conferido contra o `SHA256SUMS.txt`
# publicado na mesma release antes de qualquer `sudo`. Divergência aborta.
#
# Variáveis opcionais:
#   DBML_REPO     repositório `dono/nome` (padrão: dbmodeling/dbml)
#   DBML_VERSION  tag a instalar, ex. v0.1.4 (padrão: a última release)
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
        | sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -n 1)"
    if [ -z "$tag" ]; then
        echo "Não foi possível descobrir a última release de ${REPO}." >&2
        return 1
    fi
    echo "$tag"
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

# Permite carregar só as funções (testes): DBML_INSTALL_LIB=1 source scripts/install.sh
if [ "${DBML_INSTALL_LIB:-}" = "1" ]; then
    return 0 2>/dev/null || exit 0
fi

echo -e "${BOLD}${BLUE}"
echo "=========================================================="
echo "          dbmodeling - Assistente de Instalação          "
echo "=========================================================="
echo -e "${RESET}"

# Modo de Desinstalação
if [ "${1:-}" = "--uninstall" ] || [ "${1:-}" = "-u" ]; then
    echo -e "${YELLOW}==> Desinstalando ${DISPLAY_NAME}...${RESET}"
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
                DEB_FILE="${1:-}"
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
                RPM_FILE="${1:-}"
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
