#!/usr/bin/env bash
# Cut the live memory backend over from palimpsest to memnest. Reversible.
#
# Works even with pi running: the live pi-palimpsest extension auto-restarts
# palimpsest.service, so we neutralize that by renaming the unit + binary aside
# (so `systemctl start palimpsest` and a direct spawn both fail). memnest then
# owns :3111 unchallenged. ~/.palimpsest (the original store) is never modified.
set -euo pipefail
H="$HOME"; S="$H/.pi/agent/settings.json"; U="$H/.config/systemd/user"

echo "[cutover] 1/5 stop palimpsest + neutralize auto-resurrection"
systemctl --user stop palimpsest.service 2>/dev/null || true
systemctl --user disable palimpsest.service 2>/dev/null || true
[ -f "$U/palimpsest.service" ] && mv "$U/palimpsest.service" "$U/palimpsest.service.disabled-for-memnest"
[ -x "$H/.local/bin/palimpsest" ] && mv "$H/.local/bin/palimpsest" "$H/.local/bin/palimpsest.bak-cutover"
systemctl --user daemon-reload

echo "[cutover] 2/5 refresh ~/.memnest from ~/.palimpsest (independent copy)"
mkdir -p "$H/.memnest"
rm -f  "$H/.memnest/memory.db" "$H/.memnest"/memory.db-*
rm -rf "$H/.memnest/text_index" "$H/.memnest/vectors" "$H/.memnest/journal"
[ -e "$H/.memnest/models" ] || cp -al "$H/.palimpsest/models" "$H/.memnest/models"
cp "$H/.palimpsest/memory.db"  "$H/.memnest/memory.db"
cp "$H/.palimpsest/master.key" "$H/.memnest/master.key"   # vault key (decrypt via legacy-salt fallback)
cp -r "$H/.palimpsest/journal" "$H/.memnest/journal" 2>/dev/null || true

echo "[cutover] 3/5 swap pi extension pi-palimpsest -> pi-memnest"
node -e '
const fs=require("fs"), p=process.env.HOME+"/.pi/agent/settings.json";
const c=JSON.parse(fs.readFileSync(p,"utf8"));
fs.writeFileSync(p+".bak-memnest-cutover", fs.readFileSync(p));
c.packages=(c.packages||[]).map(x=> x==="../../pi-palimpsest" ? "../../memnest/pi-extension" : x);
fs.writeFileSync(p, JSON.stringify(c,null,2));
console.log("    settings.json updated (backup: settings.json.bak-memnest-cutover)");'

echo "[cutover] 4/5 enable + start memnest.service (first start rebuilds index, ~3 min)"
systemctl --user unmask memnest.service 2>/dev/null || true
systemctl --user enable --now memnest.service

echo "[cutover] 5/5 wait for memnest health on :3111"
for i in $(seq 1 120); do
  curl -s -m2 -o /dev/null http://127.0.0.1:3111/health 2>/dev/null && { echo "    memnest healthy"; ok=1; break; }
  sleep 2
done
[ "${ok:-}" = 1 ] || { echo "    !! not healthy — journalctl --user -u memnest -n 30"; exit 1; }
echo "[cutover] DONE. memnest live on :3111. Restart pi to load pi-memnest. Rollback: rollback-to-palimpsest.sh"
