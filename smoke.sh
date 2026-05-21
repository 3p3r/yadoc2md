#!/usr/bin/env bash
# Smoke-test yadoc2md against fixtures/ in CLI and REST modes.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$ROOT"

BIN="${BIN:-$ROOT/target/debug/yadoc2md}"
FIXTURES_DIR="$ROOT/fixtures"

pick_port() {
  python3 -c 'import socket; s=socket.socket(); s.bind(("", 0)); print(s.getsockname()[1]); s.close()'
}

PORT="${PORT:-$(pick_port)}"
BASE_URL="http://127.0.0.1:${PORT}"

# Fixtures that must convert successfully (extension supported by anytomd or unpdf).
EXPECT_OK=(
  sample.docx
  sample.html
  sample.js
  sample.json
  sample.md
  sample.pdf
  sample.png
  sample.pptx
  sample.py
  sample.rs
  sample.sh
  sample.svg
  sample.toml
  sample.ts
  sample.txt
  sample.xlsx
)

# Fixtures that must fail conversion (unsupported extension).
EXPECT_FAIL=(
  sample.css
  sample.mp4
  sample.wav
)

SERVER_PID=""
SMOKE_LOG="${TMPDIR:-/tmp}/yadoc2md-smoke-${PORT}.log"

stop_server() {
  if [[ -n "${SERVER_PID}" ]] && kill -0 "${SERVER_PID}" 2>/dev/null; then
    kill -TERM "${SERVER_PID}" 2>/dev/null || true
    local i=0
    while kill -0 "${SERVER_PID}" 2>/dev/null && (( i < 30 )); do
      sleep 0.1
      i=$((i + 1))
    done
    if kill -0 "${SERVER_PID}" 2>/dev/null; then
      kill -KILL "${SERVER_PID}" 2>/dev/null || true
    fi
    wait "${SERVER_PID}" 2>/dev/null || true
  fi
  SERVER_PID=""

  # Fallback: anything still bound to our test port (e.g. script killed with SIGKILL).
  if command -v fuser >/dev/null 2>&1; then
    fuser -k "${PORT}/tcp" 2>/dev/null || true
  elif command -v lsof >/dev/null 2>&1; then
    local pids
    pids="$(lsof -ti ":${PORT}" 2>/dev/null || true)"
    if [[ -n "$pids" ]]; then
      kill -TERM $pids 2>/dev/null || true
      sleep 0.3
      kill -KILL $pids 2>/dev/null || true
    fi
  fi

  rm -f "$SMOKE_LOG"
}

cleanup() {
  stop_server
}

trap cleanup EXIT INT TERM

log() { printf '\n==> %s\n' "$*"; }
fail() { echo "FAIL: $*" >&2; exit 1; }

require_fixture() {
  local name=$1
  [[ -f "$FIXTURES_DIR/$name" ]] || fail "missing fixture: $FIXTURES_DIR/$name"
}

build_binary() {
  log "building yadoc2md"
  cargo build -q
  [[ -x "$BIN" ]] || fail "binary not found: $BIN"
}

cli_smoke() {
  log "CLI smoke tests"
  local name out code

  for name in "${EXPECT_OK[@]}"; do
    require_fixture "$name"
    out="$(mktemp)"
    if ! "$BIN" parse "$FIXTURES_DIR/$name" >"$out"; then
      rm -f "$out"
      fail "CLI: expected success for $name (exit != 0)"
    fi
    if [[ ! -s "$out" ]]; then
      rm -f "$out"
      fail "CLI: expected non-empty markdown for $name"
    fi
    rm -f "$out"
    echo "  ok  parse $name"
  done

  for name in "${EXPECT_FAIL[@]}"; do
    require_fixture "$name"
    if "$BIN" parse "$FIXTURES_DIR/$name" >/dev/null 2>&1; then
      fail "CLI: expected failure for $name (exit 0)"
    fi
    echo "  ok  parse $name (failed as expected)"
  done
}

start_server() {
  stop_server
  log "starting REST server on port $PORT"
  "$BIN" serve --host 127.0.0.1 --port "$PORT" >"$SMOKE_LOG" 2>&1 &
  SERVER_PID=$!
  for _ in $(seq 1 50); do
    if curl -sf "$BASE_URL/api/health" >/dev/null 2>&1; then
      return 0
    fi
    if ! kill -0 "${SERVER_PID}" 2>/dev/null; then
      cat "$SMOKE_LOG" >&2 || true
      fail "server exited before becoming ready (pid ${SERVER_PID})"
    fi
    sleep 0.1
  done
  cat "$SMOKE_LOG" >&2 || true
  stop_server
  fail "server did not become ready at $BASE_URL"
}

rest_smoke() {
  log "REST smoke tests"
  local name tmp code body

  code="$(curl -s -o /dev/null -w '%{http_code}' "$BASE_URL/")"
  [[ "$code" == "307" || "$code" == "302" || "$code" == "303" ]] \
    || fail "REST: / redirect expected 302/303/307, got $code"
  echo "  ok  GET / (redirect to swagger-ui)"

  body="$(curl -sf "$BASE_URL/api/health")"
  [[ "$body" == *'"status":"ok"'* ]] || fail "REST: /api/health unexpected body: $body"
  echo "  ok  GET /api/health"

  code="$(curl -s -o /dev/null -w '%{http_code}' "$BASE_URL/api-doc/openapi.json")"
  [[ "$code" == "200" ]] || fail "REST: openapi.json returned $code"
  echo "  ok  GET /api-doc/openapi.json"

  code="$(curl -s -o /dev/null -w '%{http_code}' "$BASE_URL/swagger-ui/")"
  [[ "$code" == "200" ]] || fail "REST: swagger-ui returned $code"
  echo "  ok  GET /swagger-ui/"

  for name in "${EXPECT_OK[@]}"; do
    require_fixture "$name"
    tmp="$(mktemp)"
    code="$(curl -s -o "$tmp" -w '%{http_code}' -F "file=@$FIXTURES_DIR/$name" "$BASE_URL/api/parse")"
    if [[ "$code" != "200" ]]; then
      echo "body: $(cat "$tmp")" >&2
      rm -f "$tmp"
      fail "REST: expected 200 for $name, got $code"
    fi
    if [[ ! -s "$tmp" ]]; then
      rm -f "$tmp"
      fail "REST: expected non-empty markdown for $name"
    fi
    rm -f "$tmp"
    echo "  ok  POST /api/parse $name"
  done

  for name in "${EXPECT_FAIL[@]}"; do
    require_fixture "$name"
    tmp="$(mktemp)"
    code="$(curl -s -o "$tmp" -w '%{http_code}' -F "file=@$FIXTURES_DIR/$name" "$BASE_URL/api/parse")"
    case "$code" in
      422)
        grep -q '"error"' "$tmp" || fail "REST: expected JSON error body for $name"
        echo "  ok  POST /api/parse $name (422 conversion error)"
        ;;
      400)
        # Some binary fixtures (e.g. wav) fail multipart parsing before conversion.
        echo "  ok  POST /api/parse $name (400 rejected before convert)"
        ;;
      *)
        echo "body: $(cat "$tmp")" >&2
        rm -f "$tmp"
        fail "REST: expected 400 or 422 for $name, got $code"
        ;;
    esac
    rm -f "$tmp"
  done
}

main() {
  build_binary
  cli_smoke
  start_server
  rest_smoke
  stop_server
  log "all smoke tests passed"
}

main "$@"
