#!/usr/bin/env bash
# One-shot installer for the memnest memory stack on Linux/macOS/WSL.
#   curl -fsSL https://raw.githubusercontent.com/Blue-B/memnest/main/pi-extension/contrib/install.sh | bash
set -euo pipefail

NEED=(curl tar git)
for cmd in "${NEED[@]}"; do
	command -v "$cmd" >/dev/null 2>&1 || {
		echo "missing: $cmd"
		exit 2
	}
done

DATA="${MEMNEST_DATA_DIR:-$HOME/.memnest}"
BIN="${MEMNEST_BIN_DIR:-$HOME/.local/bin}"
mkdir -p "$DATA" "$BIN"

# 1. install memnest core binary
if ! command -v memnest >/dev/null 2>&1; then
	echo "[1/3] installing memnest core to $BIN ..."
	if command -v cargo >/dev/null 2>&1; then
		tmp="$(mktemp -d)"
		git clone --depth 1 https://github.com/Blue-B/memnest "$tmp/memnest"
		(cd "$tmp/memnest/core" && cargo build --release)
		install -Dm755 "$tmp/memnest/core/target/release/memnest" "$BIN/memnest"
		rm -rf "$tmp"
	else
		echo "    cargo not found; please install Rust then re-run, or download binary manually"
		exit 3
	fi
fi
command -v memnest >/dev/null 2>&1 || export PATH="$BIN:$PATH"

# 2. register a systemd --user service if systemd is available
if command -v systemctl >/dev/null 2>&1 && systemctl --user status >/dev/null 2>&1; then
	echo "[2/3] installing systemd --user service ..."
	mkdir -p "$HOME/.config/systemd/user"
	cat >"$HOME/.config/systemd/user/memnest.service" <<EOF
[Unit]
Description=memnest memory server
After=default.target

[Service]
ExecStart=$BIN/memnest --host 127.0.0.1 --port 3111
Environment=MEMNEST_DATA_DIR=$DATA
Restart=on-failure
RestartSec=2s

[Install]
WantedBy=default.target
EOF
	systemctl --user daemon-reload
	systemctl --user enable --now memnest
else
	echo "[2/3] systemd --user not available, skipping service install"
	echo "       start manually: memnest &"
fi

# 3. install pi-memnest extension from source (if pi is present)
# pi-memnest is not published to npm yet, so build it from a checkout.
if command -v pi >/dev/null 2>&1; then
	echo "[3/3] installing pi-memnest extension ..."
	tmp_ext="$(mktemp -d)"
	git clone --depth 1 https://github.com/Blue-B/memnest "$tmp_ext/memnest"
	(cd "$tmp_ext/memnest/pi-extension" && npm install && pi install .)
	rm -rf "$tmp_ext"
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
echo "  - Register memnest --mcp in your AI client: see https://github.com/Blue-B/memnest#connect-your-agent"
echo "  - Mirror to a git repo: npm install -g ./journal && pjournal init ~/memory-journal"
