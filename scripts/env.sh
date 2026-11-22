# source scripts/env.sh —— 本地构建环境。所有路径都可通过同名环境变量覆盖。
: "${OFFTABLE_TOOLCHAINS:=/mnt/data/toolchains}"
: "${OFFTABLE_CACHE:=/mnt/data/cache}"
export CARGO_HOME="${CARGO_HOME:-$OFFTABLE_TOOLCHAINS/cargo}"
export RUSTUP_HOME="${RUSTUP_HOME:-$OFFTABLE_TOOLCHAINS/rustup}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$OFFTABLE_CACHE/target/offtable}"
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}"
export JAVA_HOME="${JAVA_HOME:-$OFFTABLE_TOOLCHAINS/jdk-21}"
export ANDROID_HOME="${ANDROID_HOME:-$OFFTABLE_TOOLCHAINS/android-sdk}"
export ANDROID_SDK_ROOT="$ANDROID_HOME"
if [ -z "${NDK_HOME:-}" ] && [ -d "$ANDROID_HOME/ndk" ]; then
  export NDK_HOME="$(ls -d "$ANDROID_HOME"/ndk/* 2>/dev/null | sort -V | tail -1)"
fi
export GRADLE_USER_HOME="${GRADLE_USER_HOME:-$OFFTABLE_CACHE/gradle}"
export GRADLE_OPTS="${GRADLE_OPTS:--Dorg.gradle.daemon=false -Dorg.gradle.workers.max=2 -Xmx3g}"
export PATH="$OFFTABLE_TOOLCHAINS/bin:$CARGO_HOME/bin:$JAVA_HOME/bin:$ANDROID_HOME/cmdline-tools/latest/bin:$ANDROID_HOME/platform-tools:$PATH"
