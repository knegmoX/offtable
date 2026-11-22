#!/usr/bin/env bash
# 安装与 Cargo.lock 中 wasm-bindgen 版本一致的 wasm-bindgen CLI，以及 binaryen 的 wasm-opt。
set -euo pipefail
WB=0.2.130
BINARYEN=version_133
DEST="${1:-$HOME/.local/bin}"
mkdir -p "$DEST" /tmp/wasm-tools && cd /tmp/wasm-tools
case "$(uname -s)-$(uname -m)" in
  Linux-x86_64) WBT=x86_64-unknown-linux-musl; BNT=x86_64-linux ;;
  Darwin-arm64) WBT=aarch64-apple-darwin; BNT=arm64-macos ;;
  Darwin-x86_64) WBT=x86_64-apple-darwin; BNT=x86_64-macos ;;
  *) echo "unsupported host"; exit 1 ;;
esac
curl -sfL "https://github.com/wasm-bindgen/wasm-bindgen/releases/download/$WB/wasm-bindgen-$WB-$WBT.tar.gz" | tar xz
cp "wasm-bindgen-$WB-$WBT/wasm-bindgen" "$DEST/"
curl -sfL "https://github.com/WebAssembly/binaryen/releases/download/$BINARYEN/binaryen-$BINARYEN-$BNT.tar.gz" | tar xz
cp "binaryen-$BINARYEN/bin/wasm-opt" "$DEST/"
mkdir -p "$DEST/../lib" && cp -R "binaryen-$BINARYEN/lib/." "$DEST/../lib/" 2>/dev/null || true
"$DEST/wasm-bindgen" --version
"$DEST/wasm-opt" --version
