#!/usr/bin/env bash
# APK 体积报告 + 门禁（arm64 安装包硬上限 8 MB），输出到 $GITHUB_STEP_SUMMARY。
set -euo pipefail
LIMIT=$((8 * 1024 * 1024))
out="### Android 包体\n\n| 文件 | APK | 原生库 | gzip 估算（≈下载） |\n|---|---|---|---|\n"
fail=0
for apk in "$@"; do
  size=$(stat -c %s "$apk")
  so=$(unzip -l "$apk" | awk '/lib\/.*\.so$/ {s+=$1} END {print s+0}')
  gz=$(gzip -9 -c "$apk" | wc -c)
  out+="| $(basename "$apk") | $((size / 1024)) KB | $((so / 1024)) KB | $((gz / 1024)) KB |\n"
  if [[ "$apk" == *arm64* || "$apk" == *aarch64* ]] && (( size > LIMIT )); then fail=1; fi
done
printf "%b" "$out"
[ -n "${GITHUB_STEP_SUMMARY:-}" ] && printf "%b\n" "$out" >> "$GITHUB_STEP_SUMMARY"
if (( fail )); then echo "arm64 APK 超过 8 MB 上限"; exit 1; fi
