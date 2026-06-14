#!/usr/bin/env bash
# Reverse cutover-to-memnest.sh: restore palimpsest as the live backend.
#
# NOTE: memories captured WHILE on memnest live in ~/.memnest and are NOT merged
# back into ~/.palimpsest (independent stores). Rolling back returns you to the
# palimpsest store as of cutover time.
set -euo pipefail
H="$HOME"; S="$H/.pi/agent/settings.json"; U="$H/.config/systemd/user"

echo "[rollback] 1/4 stop + disable memnest"
systemctl --user stop memnest.service 2>/dev/null || true
systemctl --user disable memnest.service 2>/dev/null || true

echo "[rollback] 2/4 restore palimpsest unit + binary"
[ -f "$U/palimpsest.service.disabled-for-memnest" ] && mv "$U/palimpsest.service.disabled-for-memnest" "$U/palimpsest.service"
[ -x "$H/.local/bin/palimpsest.bak-cutover" ] && mv "$H/.local/bin/palimpsest.bak-cutover" "$H/.local/bin/palimpsest"
systemctl --user daemon-reload

echo "[rollback] 3/4 restore pi extension (settings.json)"
if [ -f "$S.bak-memnest-cutover" ]; then cp "$S.bak-memnest-cutover" "$S"; echo "    restored from backup";
else node -e '
  const fs=require("fs"), p=process.env.HOME+"/.pi/agent/settings.json";
  const c=JSON.parse(fs.readFileSync(p,"utf8"));
  c.packages=(c.packages||[]).map(x=> x==="../../memnest/pi-extension" ? "../../pi-palimpsest" : x);
  fs.writeFileSync(p, JSON.stringify(c,null,2));
  console.log("    settings.json reverted (no backup found)");'
fi

echo "[rollback] 4/4 enable + start palimpsest"
systemctl --user enable --now palimpsest.service
for i in $(seq 1 60); do curl -s -m2 -o /dev/null http://127.0.0.1:3111/health 2>/dev/null && { echo "    palimpsest healthy"; break; }; sleep 2; done
echo "[rollback] DONE. Restart pi to load pi-palimpsest again."
