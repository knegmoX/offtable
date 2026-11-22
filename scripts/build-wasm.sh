#!/usr/bin/env bash
# 构建 game.wasm → web/src/wasm/pkg（wasm-bindgen --target web + wasm-opt -Oz）
set -euo pipefail
cd "$(dirname "$0")/.."
[ -n "${CI:-}" ] || source scripts/env.sh
OUT=web/src/wasm/pkg
scripts/lp cargo build -q -p offtable-wasm --target wasm32-unknown-unknown --profile wasm
WASM="${CARGO_TARGET_DIR:-target}/wasm32-unknown-unknown/wasm/offtable_wasm.wasm"
rm -rf "$OUT" && mkdir -p "$OUT"
wasm-bindgen --target web --no-typescript --out-dir "$OUT" --out-name game "$WASM"
scripts/lp wasm-opt -Oz --enable-bulk-memory --enable-nontrapping-float-to-int --enable-sign-ext --enable-mutable-globals -o "$OUT/game_bg.opt.wasm" "$OUT/game_bg.wasm"
mv "$OUT/game_bg.opt.wasm" "$OUT/game_bg.wasm"
node scripts/brsize.mjs "$OUT/game_bg.wasm" "$OUT/game.js"
