#!/usr/bin/env bash
# Remote Android build for the Todo app (<remote-host>, Debian 13).
# Run on the remote host: bash ~/remote-android-build.sh
set -e
cd "$HOME"
mkdir -p "$HOME/todo-app"
tar -xzf "$HOME/todo-app-source-20261111.tar.gz" -C "$HOME/todo-app"
ls "$HOME/todo-app/src-tauri/gen/android/build.gradle.kts"
ls "$HOME/todo-app/src-tauri/gen/android/app/src/main/java/" 2>/dev/null || true

cat > "$HOME/android-env.sh" <<'ENVEOF'
export JAVA_HOME=/usr/lib/jvm/java-21-openjdk-amd64
export ANDROID_HOME=$HOME/android-sdk
export ANDROID_SDK_ROOT=$ANDROID_HOME
export NDK_HOME=$ANDROID_HOME/ndk/26.3.11579264
export ANDROID_NDK_HOME=$NDK_HOME
export GRADLE_USER_HOME=$HOME/.gradle
export PATH=$HOME/.cargo/bin:/usr/local/bin:$JAVA_HOME/bin:$ANDROID_HOME/cmdline-tools/latest/bin:$ANDROID_HOME/platform-tools:$PATH
ENVEOF
mkdir -p "$HOME/.gradle"
printf 'org.gradle.daemon=false\n' > "$HOME/.gradle/gradle.properties"

# shellcheck disable=SC1091
source "$HOME/android-env.sh"
export GRADLE_OPTS="-Xmx2048m -Dfile.encoding=UTF-8"
cd "$HOME/todo-app"
java -version
cargo ndk --version
# Local toolchain pins pnpm 10 (pnpm 12 refuses our lockfile: minimum-release-age policy)
if [ ! -x "$HOME/.local/share/pnpm/pnpm" ]; then
  curl -fsSL https://get.pnpm.io/install.sh | env PNPM_VERSION=10.34.9 sh -
fi
export PNPM_HOME="$HOME/.local/share/pnpm"
export PATH="$PNPM_HOME:$PATH"
pnpm --version
pnpm install
df -h "$HOME" | tail -1
pnpm tauri android build --apk -d -t aarch64 --ci 2>&1 | tee "$HOME/todo-app-build.log"
BUILD_EXIT=${PIPESTATUS[0]}
echo "BUILD-EXIT=$BUILD_EXIT"
find "$HOME/todo-app" -name "*.apk" 2>/dev/null
df -h "$HOME" | tail -1
exit "$BUILD_EXIT"
