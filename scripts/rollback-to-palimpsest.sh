#!/usr/bin/env bash
# Roll the live memory backend back from memnest to palimpsest.
# Run with pi NOT running, then start pi again.
#
# NOTE: any memories captured WHILE on memnest live in ~/.memnest and are NOT
# merged back into ~/.palimpsest (the two stores are independent). Rolling back
# returns you to the palimpsest store as it was at cutover time.
set -euo pipefail
H="$HOME"; S="$H/.pi/agent/settings.json"

echo "[rollback] stop + disable memnest"
systemctl --user stop memnest.service 2>/dev/null || true
systemctl --user disable memnest.service 2>/dev/null || true

echo "[rollback] restore pi extension (settings.json)"
if [ -f "$S.bak-cutover" ]; then
  cp "$S.bak-cutover" "$S"
  echo "    restored from settings.json.bak-cutover"
else
  node -e '
  const fs=require("fs"), p=process.env.HOME+"/.pi/agent/settings.json";
  const c=JSON.parse(fs.readFileSync(p,"utf8"));
  c.packages=(c.packages||[]).map(x=> x==="../../memnest/pi-extension" ? "../../pi-palimpsest" : x);
  fs.writeFileSync(p, JSON.stringify(c,null,2));
  console.log("    settings.json reverted (no backup found)");'
fi

echo "[rollback] enable + start palimpsest"
systemctl --user enable --now palimpsest.service
for i in $(seq 1 60); do curl -s -m2 -o /dev/null http://127.0.0.1:3111/health 2>/dev/null && { echo "    palimpsest healthy"; break; }; sleep 2; done

echo "[rollback] DONE. Now START pi (it will load pi-palimpsest again)."
