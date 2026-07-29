#!/usr/bin/env bash
set -euo pipefail

REPO_URL="https://github.com/fishing-ainpm/Totr-.git"
INSTALL_DIR="${ANTIBOT_INSTALL_DIR:-$HOME/.local/share/antibot}"

echo "== Security Protective Botnet Defensive — install =="

if ! command -v cargo >/dev/null 2>&1; then
    if [ -n "${TERMUX_VERSION:-}" ]; then
        echo "Termux detectado — instalando rust via pkg..."
        pkg update -y && pkg install -y rust git
    else
        echo "cargo não encontrado — instalando Rust via rustup..."
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
        # shellcheck disable=SC1090
        source "$HOME/.cargo/env"
    fi
fi

if ! command -v git >/dev/null 2>&1; then
    echo "git não encontrado — instala git antes de rodar isso." >&2
    exit 1
fi

if [ -d "$INSTALL_DIR/.git" ]; then
    echo "atualizando instalação existente em $INSTALL_DIR"
    git -C "$INSTALL_DIR" pull --ff-only
else
    echo "clonando pra $INSTALL_DIR"
    git clone --depth 1 "$REPO_URL" "$INSTALL_DIR"
fi

cd "$INSTALL_DIR"
cargo install --path . --force

echo
echo "instalado. Próximos passos:"
echo "  antibot setup   # cria antibot.toml, .gitignore, git init — um comando só"
echo "  antibot         # modo automático: /start /manutein /logout"
