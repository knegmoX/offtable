#!/usr/bin/env bash
# 一次性安装 JDK 21 + Android SDK/NDK 到 $OFFTABLE_TOOLCHAINS（经 scripts/lp 低优先级运行）
set -euo pipefail
source "$(dirname "$0")/env.sh"
DL=/tmp/offtable-dl; mkdir -p "$DL"
if [ ! -x "$JAVA_HOME/bin/java" ]; then
  curl -sfL -o "$DL/jdk.tgz" "https://github.com/adoptium/temurin21-binaries/releases/download/jdk-21.0.13.1%2B2/OpenJDK21U-jdk_x64_linux_hotspot_21.0.13.1_2.tar.gz"
  mkdir -p "$JAVA_HOME" && tar xzf "$DL/jdk.tgz" -C "$JAVA_HOME" --strip-components=1
fi
java -version 2>&1 | head -1
if [ ! -x "$ANDROID_HOME/cmdline-tools/latest/bin/sdkmanager" ]; then
  curl -sfL -o "$DL/clt.zip" "https://dl.google.com/android/repository/commandlinetools-linux-16111833_latest.zip"
  mkdir -p "$ANDROID_HOME/cmdline-tools" && rm -rf "$DL/clt" && unzip -q "$DL/clt.zip" -d "$DL/clt"
  rm -rf "$ANDROID_HOME/cmdline-tools/latest" && mv "$DL/clt/cmdline-tools" "$ANDROID_HOME/cmdline-tools/latest"
fi
yes | sdkmanager --licenses >/dev/null 2>&1 || true
sdkmanager --install "platform-tools" "platforms;android-36" "platforms;android-37.0" "build-tools;36.1.0" "ndk;28.2.13676358" 2>&1 | grep -v '^\[=*' | tail -5
sdkmanager --list_installed 2>/dev/null | tail -8
rm -f "$DL/jdk.tgz" "$DL/clt.zip"
echo SETUP_ANDROID_DONE
