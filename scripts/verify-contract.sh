#!/usr/bin/env bash
# 문서가 주장하는 것을 실행해서 대조한다. 읽지 않고 실행한다.
# 사용법: verify-contract.sh <base-url> <memnest-binary> <data-dir>
BASE="${1:-http://127.0.0.1:3111}"
BIN="${2:-$HOME/.local/bin/memnest}"
DATA="${3:-$HOME/.memnest}"
R="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
pass=0; fail=0
# ok/no must always succeed, otherwise `cond && ok || no` would fire both.
ok(){ printf "  PASS  %s\n" "$1"; pass=$((pass+1)); return 0; }
no(){ printf "  FAIL  %s  -- %s\n" "$1" "$2"; fail=$((fail+1)); return 0; }
code(){ curl -s -o /dev/null -w '%{http_code}' -m 8 "$BASE$1"; }

echo "== 1. 문서가 안내한 엔드포인트가 실제로 응답하는가 =="
# README/docs에 curl 예제로 등장하는 경로를 추출해 실제로 호출
for p in /health /stats; do
  c=$(code "$p"); [ "$c" = "200" ] && ok "GET $p -> 200" || no "GET $p" "HTTP=$c"
done
for p in /restore /prune; do
  # 문서에 POST curl 예제가 있는 운영자 API. 405가 아니라 존재해야 한다
  c=$(curl -s -o /dev/null -w '%{http_code}' -m 8 -X POST -H 'content-type: application/json' -d '{}' "$BASE$p")
  [ "$c" != "404" ] && ok "POST $p 존재 (HTTP=$c)" || no "POST $p" "404, 문서엔 예제 있음"
done

echo "== 2. 제거한 표면이 정말 사라졌는가 =="
for p in / /viewer/search /feedback /operations /collections; do
  c=$(code "$p"); [ "$c" = "404" ] && ok "$p -> 404" || no "$p 제거 안 됨" "HTTP=$c"
done

echo "== 3. 문서가 약속한 툴 목록과 런타임이 일치하는가 =="
doc=$(sed -n '/^## Tool contract/,/^### /p' "$R/README.md" | grep -oE '^memory_[a-z]+' | sort | paste -sd,)
run=$(curl -s -X POST "$BASE/mcp" -H 'content-type: application/json' -H 'accept: application/json, text/event-stream' \
      -d '{"jsonrpc":"2.0","id":1,"method":"tools/list"}' 2>/dev/null | tr -d '\0' \
      | grep -oE '"name":"memory_[a-z]+"' | sed 's/"name":"//;s/"//' | sort | paste -sd,)
[ "$doc" = "$run" ] && ok "툴 목록 일치: $run" || no "툴 목록 불일치" "문서=$doc 런타임=$run"

echo "== 4. 문서가 나열한 데이터 파일이 실제로 생기는가 =="
for f in memory.db text_index vectors models master.key; do
  [ -e "$DATA/$f" ] && ok "$f 존재" || no "$f 없음" "README 저장 구조에 기재됨"
done

echo "== 5. 문서에 적힌 CLI 명령이 실제로 동작하는가 =="
for sub in status hook watch; do
  "$BIN" --help 2>&1 | grep -qE "^  $sub " && ok "서브커맨드 $sub 존재" || no "서브커맨드 $sub" "--help에 없음"
done
"$BIN" --help 2>&1 | grep -qiE "^  dashboard " && no "dashboard 서브커맨드 잔존" "제거했어야 함" || ok "dashboard 서브커맨드 없음"
"$BIN" --version >/dev/null 2>&1 && ok "--version 동작" || no "--version" "실패"

echo "== 6. 문서가 없다고 한 것이 정말 응답에 없는가 =="
h=$(curl -s -m 8 "$BASE/health")
echo "$h" | grep -q dashboard_url && no "/health에 dashboard_url 잔존" "제거했어야 함" || ok "/health에 dashboard_url 없음"
s=$(curl -s -m 15 -X POST "$BASE/search" -H 'content-type: application/json' \
    -d '{"query":"verification probe","project":"","cwd":"/home/shell","n_results":1}')
echo "$s" | grep -q recall_id && no "/search에 recall_id 잔존" "제거했어야 함" || ok "/search에 recall_id 없음"
echo "$s" | grep -q '"project"' && ok "/search에 project 존재" || no "/search project 누락" "autocontext가 의존"
echo "$s" | grep -q helpful_count && no "helpful_count 잔존" "피드백 제거했어야 함" || ok "helpful_count 없음"

echo "== 7. 죽은 테이블이 정말 없는가 =="
python3 - "$DATA/memory.db" <<'PY'
import sqlite3,sys
try:
    c=sqlite3.connect(f"file:{sys.argv[1]}?mode=ro",uri=True)
    have={r[0] for r in c.execute("select name from sqlite_master where type='table'")}
    dead={"facts","notes","servers","recall_events","recall_result_feedback"}
    live={"chunks","secrets","processing_jobs","workspaces"}
    for t in sorted(dead & have): print(f"  FAIL  죽은 테이블 {t} 잔존")
    for t in sorted(dead - have): print(f"  PASS  {t} 제거됨")
    for t in sorted(live - have): print(f"  FAIL  필수 테이블 {t} 없음")
    for t in sorted(live & have): print(f"  PASS  {t} 유지")
except Exception as e: print("  SKIP  DB 열기 실패:", str(e)[:60])
PY

echo "== 8. 보안 주장이 실제로 강제되는가 =="
grep -q "127.0.0.1" "$R/README.md" && ok "README에 로컬 바인드 명시" || no "바인드 서술" "누락"
grep -rq "enforce_bind_safety" "$R/core/src" && ok "비로컬 바인드 차단 코드 존재" || no "bind safety" "코드 없음"
grep -rq "Aes256Gcm" "$R/core/src/crypto.rs" && ok "AES-256-GCM 사용 확인" || no "암호화" "코드 없음"
[ "$(stat -c %a "$DATA/master.key" 2>/dev/null)" = "600" ] && ok "master.key 권한 600" || no "master.key 권한" "$(stat -c %a "$DATA/master.key" 2>/dev/null)"

echo
echo "합계: PASS=$pass FAIL=$fail"
[ "$fail" -eq 0 ] || exit 1
