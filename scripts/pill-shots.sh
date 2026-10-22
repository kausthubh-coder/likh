#!/usr/bin/env bash
# The pill on a real macOS screen, for CI (macOS runners have a desktop):
# screenshots before a dictation, while listening, on a failure, and after it
# should be gone, into the current folder as pill-*.png. Fakes as in
# e2e-dictate.sh; the first transcription fails (flaky:) so the error shows.
#
#   scripts/pill-shots.sh <likh binary> <speech.wav>
set -euo pipefail
likh="$(cd "$(dirname "$1")" && pwd)/$(basename "$1")"
wav="$(cd "$(dirname "$2")" && pwd)/$(basename "$2")"
root="$(cd "$(dirname "$0")/.." && pwd)"

work="$(mktemp -d)"
export LIKH_HOME="$work/home"
mkdir -p "$LIKH_HOME"
echo '{ "auto_update": false, "pause_media": false, "sounds": false }' > "$LIKH_HOME/settings.json"
codex="$(cd "$root" && bun scripts/fake-codex-home.mjs connected | tail -1)"

LIKH_AUDIO="$wav" LIKH_TRANSCRIBER="flaky:Send the launch notes to the team by Friday." \
  LIKH_PASTE="$work/pasted.jsonl" CODEX_HOME="$codex" \
  "$likh" run --skip-onboarding > "$work/likh.log" 2>&1 < /dev/null &
pid=$!
trap '"$likh" ctl quit > /dev/null 2>&1 || true; sleep 1; kill "$pid" 2>/dev/null || true' EXIT
for _ in $(seq 1 60); do "$likh" ctl status > /dev/null 2>&1 && break; sleep 0.5; done

screencapture -x pill-0-before.png
"$likh" ctl start > /dev/null
sleep 2
screencapture -x pill-1-listening.png
"$likh" ctl stop > /dev/null
sleep 1.5
screencapture -x pill-2-error.png
sleep 8
screencapture -x pill-3-gone.png
"$likh" ctl status
cat "$work/likh.log"
