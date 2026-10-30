#!/usr/bin/env bash
# Renders the README screenshots from synthetic demo data: the Rust core
# builds a demo dashboard, and the UI runs in headless Chromium with the
# Tauri calls faked (app/harness/). Nothing real is ever shown.
#
#   scripts/screenshots.sh     writes docs/screenshots/*.png
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"
out=docs/screenshots
chrome=${CHROMIUM:-chromium}
port=5199
mkdir -p "$out"

cargo run -q -p sachiva-export --example demo >app/harness/demo.json
(cd app && exec npx vite --port $port --strictPort >/dev/null 2>&1) &
vite=$!
trap 'kill $vite 2>/dev/null || true' EXIT
for _ in $(seq 40); do curl -sf "http://localhost:$port/" >/dev/null && break; sleep 0.25; done

profile="$(mktemp -d)"
shot() { # name width,height hash
  rm -rf "$profile"/*
  LANG=en_GB.UTF-8 "$chrome" --headless=new --lang=en-GB --disable-gpu --no-sandbox --hide-scrollbars \
    --user-data-dir="$profile" --window-size="$2" --virtual-time-budget=4000 \
    --screenshot="$out/$1.png" "http://localhost:$port/harness/index.html#$3" >/dev/null 2>&1
  echo "$out/$1.png"
}
shot dashboard 1280,860 "x=1"
shot dashboard-dark 1280,860 "theme=dark"
shot phone 390,844 "platform=android"
shot tasks 1280,860 "view=tasks"
shot editor 1280,1000 "edit=1"
shot recovery 1280,760 "mode=first"
shot setup 1280,760 "mode=setup"
shot update 1280,860 "update=1"
shot portable 1280,860 "update=1&kind=portable&platform=windows"
shot settings 1280,2400 "view=settings"
shot waiting-lists 1280,1100 "view=lists"
shot report-waiting 1280,2600 "view=reports&report=waiting"
shot report-performance 1280,1800 "view=reports&report=performance"
rm -rf "$profile"
