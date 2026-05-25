#!/usr/bin/env bash
# One-shot installer for the palimpsest memory stack on Linux/macOS/WSL.
#   curl -fsSL https://raw.githubusercontent.com/Blue-B/pi-palimpsest/main/contrib/install.sh | bash
set -euo pipefail

NEED=(curl tar git)
for cmd in "${NEED[@]}"; do
  command -v "$cmd" >/dev/null 2>&1 || { echo "missing: $cmd"; exit 2; }
done

DATA="${PALIMPSEST_DATA_DIR:-$HOME/.palimpsest}"
BIN="${PALIMPSEST_BIN_DIR:-$HOME/.local/bin}"
mkdir -p "$DATA" "$BIN"

# 1. install palimpsest core binary
if ! command -v palimpsest >/dev/null 2>&1; then
  echo "[1/3] installing palimpsest core to $BIN ..."
  if command -v cargo >/dev/null 2>&1; then
    cargo install --git https://github.com/badlogic/palimpsest --root "$HOME/.local"
  else
    echo "    cargo not found; please install Rust then re-run, or download binary manually"
    exit 3
  fi
fi
command -v palimpsest >/dev/null 2>&1 || export PATH="$BIN:$PATH"

# 2. register a systemd --user service if systemd is available
if command -v systemctl >/dev/null 2>&1 && systemctl --user status >/dev/null 2>&1; then
  echo "[2/3] installing systemd --user service ..."
  mkdir -p "$HOME/.config/systemd/user"
  cat > "$HOME/.config/systemd/user/palimpsest.service" <<EOF
[Unit]
Description=palimpsest memory server
After=default.target

[Service]
ExecStart=$BIN/palimpsest --host 127.0.0.1 --port 3111
Environment=PALIMPSEST_DATA_DIR=$DATA
Restart=on-failure
RestartSec=2s

[Install]
WantedBy=default.target
EOF
  systemctl --user daemon-reload
  systemctl --user enable --now palimpsest
else
  echo "[2/3] systemd --user not available, skipping service install"
  echo "       start manually: palimpsest &"
fi

# 3. install pi-palimpsest extension (if pi is present)
if command -v pi >/dev/null 2>&1; then
  echo "[3/3] installing pi-palimpsest extension ..."
  pi install npm:pi-palimpsest || npm install -g pi-palimpsest
else
  echo "[3/3] pi not installed, skipping extension"
fi

echo
echo "✓ done"
echo
echo "Health check:"
sleep 1
curl -fsS http://127.0.0.1:3111/health || echo "  (server may need a few more seconds)"
echo
echo "Next steps:"
echo "  - Register palimpsest --mcp in your AI client: see INSTALL-CLIENTS.md"
echo "  - Mirror to a git repo: npm i -g palimpsest-journal && pjournal init ~/memory-journal"
