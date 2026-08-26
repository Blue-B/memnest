#!/usr/bin/env bash
# Runs what CI would run, locally, in one command.
#
# Exists because CI is not a reliable gate here: GitHub Actions bills private
# repositories, and a failed payment stops every job before its first step, so
# a run can go red without a single test having executed. Nothing in the API
# response distinguishes that from a real failure at a glance.
#
# Usage: scripts/preflight.sh [--skip-ci-status]
set -uo pipefail
R="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PORT="${PREFLIGHT_PORT:-3151}"
SCRATCH="$(mktemp -d -t memnest-preflight-XXXXXX)"
BIN="$R/core/target/release/memnest"
SRV=""
fails=0

step(){ printf "\n\033[1m== %s\033[0m\n" "$1"; }
run(){ # run <label> <cmd...>
  local label="$1"; shift
  if "$@" >/tmp/preflight-step.log 2>&1; then
    printf "  PASS  %s\n" "$label"
  else
    printf "  FAIL  %s\n" "$label"; tail -15 /tmp/preflight-step.log | sed 's/^/        /'
    fails=$((fails+1))
  fi
  return 0
}
cleanup(){
  # Stop the server before removing its data directory, otherwise it keeps
  # writing index files into the tree being deleted and rmdir loses the race.
  if [ -n "$SRV" ]; then kill "$SRV" 2>/dev/null; wait "$SRV" 2>/dev/null; fi
  rm -rf "$SCRATCH"
}
trap cleanup EXIT

step "CI status (informational)"
if [ "${1:-}" != "--skip-ci-status" ]; then
  tok=$(grep -m1 "github.com" "$HOME/.git-credentials" 2>/dev/null | sed -E 's|https://[^:]*:([^@]*)@.*|\1|')
  if [ -n "${tok:-}" ]; then
    curl -s -H "Authorization: Bearer $tok" \
      "https://api.github.com/repos/Blue-B/memnest/actions/runs?per_page=3&event=push" \
      | python3 -c '
import sys, json
try:
    runs = json.load(sys.stdin).get("workflow_runs", [])
except Exception:
    runs = []
if not runs:
    print("  (no runs visible)")
for r in runs:
    print("  %-10s %-26s %s" % (r["conclusion"], r["name"][:24], r["head_sha"][:7]))
print("  NOTE: a job that fails in seconds with no steps is a billing stop, not a test failure.")
'
  else
    echo "  (no token in ~/.git-credentials; skipping)"
  fi
fi

step "Build and unit tests"
run "cargo build --release" bash -c "cd '$R/core' && cargo build --release"
run "cargo test" bash -c "cd '$R/core' && cargo test --locked -- --test-threads=1"

step "Scratch service on port $PORT"
if [ -x "$BIN" ]; then
  "$BIN" --data-dir "$SCRATCH" --port "$PORT" >/tmp/preflight-server.log 2>&1 &
  SRV=$!
  up=""
  # Bounded wait: a cold start builds indexes, but an empty store is fast.
  for _ in $(seq 1 12); do
    sleep 5
    [ "$(curl -s -o /dev/null -w '%{http_code}' -m 5 "http://127.0.0.1:$PORT/health")" = "200" ] && { up=1; break; }
  done
  if [ -n "$up" ]; then printf "  PASS  service healthy\n"; else
    printf "  FAIL  service did not become healthy in 60s\n"; tail -10 /tmp/preflight-server.log | sed 's/^/        /'
    fails=$((fails+1))
  fi
else
  printf "  FAIL  %s not built\n" "$BIN"; fails=$((fails+1))
fi

step "Package suites"
run "pi-extension smoke" bash -c "cd '$R/pi-extension' && npm run smoke"
run "pi-extension e2e"   bash -c "cd '$R/pi-extension' && npm run e2e"
run "adapters"           bash -c "cd '$R/adapters/generic-http' && node test.mjs"

step "Documentation contract"
# verify-contract asserts on a /search response body, and a search against an
# empty store returns nothing at all. The journal smoke test used to leave rows
# here as a side effect; seed one explicitly now that it does not.
curl -s -m 30 -X POST "http://127.0.0.1:$PORT/add" -H 'content-type: application/json' \
  -d "{\"text\":\"preflight verification probe\",\"cwd\":\"$HOME\",\"metadata\":{\"chunk_type\":\"manual\"}}" >/dev/null
run "verify-contract (scratch)" bash "$R/scripts/verify-contract.sh" "http://127.0.0.1:$PORT" "$BIN" "$SCRATCH"

printf "\n"
if [ "$fails" -eq 0 ]; then printf "\033[1mpreflight: all checks passed\033[0m\n"; else
  printf "\033[1mpreflight: %d check(s) failed\033[0m\n" "$fails"; fi
exit "$fails"
