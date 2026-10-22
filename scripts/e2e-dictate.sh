#!/usr/bin/env bash
# One dictation end to end on Linux or macOS, with fakes for everything that
# isn't likh: a WAV for the mic, a fixed transcript, a paste log instead of
# Ctrl+V, and a throwaway LIKH_HOME and Codex sign-in. Prints what was pasted
# and exits 1 unless it's the expected text. The Windows twin is e2e-dictate.ps1.
#
#   scripts/e2e-dictate.sh <likh binary> <speech.wav> "<expected text>"
set -euo pipefail
likh="$(cd "$(dirname "$1")" && pwd)/$(basename "$1")"
wav="$(cd "$(dirname "$2")" && pwd)/$(basename "$2")"
expected="$3"
root="$(cd "$(dirname "$0")/.." && pwd)"

work="$(mktemp -d)"
export LIKH_HOME="$work/home"
mkdir -p "$LIKH_HOME"
echo '{ "auto_update": false, "pause_media": false, "sounds": false }' > "$LIKH_HOME/settings.json"
codex="$(cd "$root" && bun scripts/fake-codex-home.mjs connected | tail -1)"
paste_log="$work/pasted.jsonl"
seconds="$(python3 -c "import wave,sys; w=wave.open(sys.argv[1]); print(w.getnframes()/w.getframerate())" "$wav")"

LIKH_AUDIO="$wav" LIKH_TRANSCRIBER="echo:$expected" LIKH_PASTE="$paste_log" CODEX_HOME="$codex" \
  "$likh" run --skip-onboarding > "$work/likh.log" 2>&1 < /dev/null &
pid=$!
stop() {
  "$likh" ctl quit > /dev/null 2>&1 || true
  sleep 1
  kill "$pid" 2>/dev/null || true
}
trap stop EXIT

for _ in $(seq 1 60); do
  "$likh" ctl status > /dev/null 2>&1 && break
  sleep 0.5
done
"$likh" ctl status > /dev/null || { echo "likh never answered likh ctl:"; cat "$work/likh.log"; exit 1; }

"$likh" ctl start > /dev/null
sleep "$seconds"
"$likh" ctl stop > /dev/null
for _ in $(seq 1 40); do
  [ -s "$paste_log" ] && break
  sleep 0.25
done
pasted="$(python3 -c "import json,sys; print(json.loads(open(sys.argv[1]).readline())['paste'])" "$paste_log" 2>/dev/null || true)"
echo "pasted: $pasted"
"$likh" ctl status
if [ "$pasted" != "$expected" ]; then
  echo "expected: $expected"
  cat "$work/likh.log"
  exit 1
fi
