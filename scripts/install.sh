#!/usr/bin/env bash
# ==============================================================================
# dbmodeling - Assistente de Instalação, Atualização e Desinstalação Oficial
# Suporta: Ubuntu/Debian, Fedora/RHEL e macOS.
# Detecta e instala dependências do sistema automaticamente na hora.
# ==============================================================================
set -euo pipefail

REPO="brenoLucasN/db_modeling"
APP_NAME="db-modeling"
DISPLAY_NAME="dbmodeling"

BOLD="\033[1m"
GREEN="\033[32m"
YELLOW="\033[33m"
RED="\033[31m"
BLUE="\033[34m"
RESET="\033[0m"

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

case "$OS" in
    Darwin)
        echo -e "${BOLD}Sistema detectado: macOS (${ARCH})${RESET}"
        echo "Baixando o Assistente de Instalação Apple Package (.pkg)..."
        LATEST_URL="https://github.com/${REPO}/releases/latest/download/db-modeling-macos-setup.pkg"
        TMP_PKG="/tmp/db-modeling-setup.pkg"
        curl -fsSL "$LATEST_URL" -o "$TMP_PKG" || {
            echo -e "${RED}Erro ao baixar pacote da release. Verifique sua conexão.${RESET}"
            exit 1
        }
        echo "Instalando em /Applications..."
        sudo installer -pkg "$TMP_PKG" -target /
        rm -f "$TMP_PKG"
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

                # Verifica se arquivo local .deb foi fornecido como argumento ou busca da release
                DEB_FILE="${1:-}"
                if [ -z "$DEB_FILE" ] || [ ! -f "$DEB_FILE" ]; then
                    echo "Baixando versão mais recente do pacote .deb..."
                    DEB_FILE="/tmp/db-modeling-latest.deb"
                    curl -fsSL "https://github.com/${REPO}/releases/latest/download/db-modeling-linux-amd64.deb" -o "$DEB_FILE"
                fi

                echo "Instalando ou atualizando pacote..."
                sudo apt-get install -y "$DEB_FILE"
                echo -e "${GREEN}${BOLD}✓ ${DISPLAY_NAME} instalado/atualizado com sucesso!${RESET}"
                ;;

            fedora|rhel|centos|rocky|alma)
                echo -e "${YELLOW}==> Instalando via DNF com resolução automática de dependências...${RESET}"
                RPM_FILE="${1:-}"
                if [ -z "$RPM_FILE" ] || [ ! -f "$RPM_FILE" ]; then
                    echo "Baixando versão mais recente do pacote .rpm..."
                    RPM_FILE="/tmp/db-modeling-latest.rpm"
                    curl -fsSL "https://github.com/${REPO}/releases/latest/download/db-modeling-linux-x86_64.rpm" -o "$RPM_FILE"
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
