#!/usr/bin/env bash
# Cut the live memory backend over from palimpsest to memnest.
#
# WHY a script: the running pi session's pi-palimpsest extension auto-restarts
# palimpsest.service, so a stable cutover can't happen from inside a live pi.
# Run this with pi NOT running (quit pi first), then start pi again.
#
# Safe + reversible: ~/.palimpsest (the original store) is never modified;
# memnest runs off an independent copy in ~/.memnest. Rollback: rollback-to-palimpsest.sh
set -euo pipefail
H="$HOME"; S="$H/.pi/agent/settings.json"

echo "[cutover] 1/5 stop + disable palimpsest (original data untouched)"
systemctl --user stop palimpsest.service 2>/dev/null || true
systemctl --user disable palimpsest.service 2>/dev/null || true

echo "[cutover] 2/5 refresh ~/.memnest from ~/.palimpsest (independent copy)"
mkdir -p "$H/.memnest"
rm -f  "$H/.memnest/memory.db" "$H/.memnest/memory.db-shm" "$H/.memnest/memory.db-wal"
rm -rf "$H/.memnest/text_index" "$H/.memnest/vectors" "$H/.memnest/journal"
[ -e "$H/.memnest/models" ] || cp -al "$H/.palimpsest/models" "$H/.memnest/models"
cp "$H/.palimpsest/memory.db"  "$H/.memnest/memory.db"
cp "$H/.palimpsest/master.key" "$H/.memnest/master.key"   # vault key: keep so secrets decrypt
cp -r "$H/.palimpsest/journal" "$H/.memnest/journal" 2>/dev/null || true

echo "[cutover] 3/5 swap pi extension pi-palimpsest -> pi-memnest in settings.json"
node -e '
const fs=require("fs"), p=process.env.HOME+"/.pi/agent/settings.json";
const c=JSON.parse(fs.readFileSync(p,"utf8"));
fs.writeFileSync(p+".bak-cutover", fs.readFileSync(p));
c.packages=(c.packages||[]).map(x=> x==="../../pi-palimpsest" ? "../../memnest/pi-extension" : x);
fs.writeFileSync(p, JSON.stringify(c,null,2));
console.log("    settings.json updated (backup: settings.json.bak-cutover)");
'

echo "[cutover] 4/5 enable + start memnest.service (first start rebuilds index, ~3 min)"
systemctl --user enable --now memnest.service

echo "[cutover] 5/5 wait for memnest health on :3111"
for i in $(seq 1 120); do
  if curl -s -m2 -o /dev/null http://127.0.0.1:3111/health 2>/dev/null; then echo "    memnest healthy"; ok=1; break; fi
  sleep 2
done
[ "${ok:-}" = 1 ] || { echo "    !! memnest not healthy — check: journalctl --user -u memnest -n 30"; exit 1; }

echo
echo "[cutover] DONE. memnest is live on :3111. Now START pi (it will load pi-memnest)."
echo "          rollback any time: bash $(dirname "$0")/rollback-to-palimpsest.sh"
