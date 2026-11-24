#!/usr/bin/env bash
# Android 构建环境（第 2 轮，M6）。用法：source android-env.sh
#
# JDK / SDK / NDK 都放在 ~/workspace 下持久化，运行时重启不丢失
# （apt 装的包会被回滚，所以 JDK 不用 apt 装，见 PROGRESS.md 环境一节）。
# 版本：JDK Temurin 17.0.21 / Android SDK（platform-tools 37.0.3、
# platforms android-34 + android-36、build-tools 34.0.0 + 36.0.0、
# NDK r26d 26.3.11579264）/ cargo-ndk 4.1.2 / Gradle 9.6.1。
# 注意：gen/android 的 compileSdk/targetSdk 被降到 36，因为公开仓库
# 至今没有 android-37 平台包，待发布后再升回（见 app/build.gradle.kts）。

export JAVA_HOME="$HOME/workspace/jdk-17"
export ANDROID_HOME="$HOME/workspace/android-sdk"
export ANDROID_SDK_ROOT="$ANDROID_HOME"
export NDK_HOME="$ANDROID_HOME/ndk/26.3.11579264"
export GRADLE_USER_HOME="$HOME/.gradle"
export PATH="$HOME/.npm-global/bin:$HOME/.cargo/bin:$JAVA_HOME/bin:$ANDROID_HOME/cmdline-tools/latest/bin:$ANDROID_HOME/platform-tools:$PATH"

# Gradle 代理：本机 Java 的 HttpURLConnection 走代理隧道会失败，但 Gradle
# 下载依赖用 Apache HttpClient，配 systemProp 代理即可。从 $https_proxy
# 解析主机端口，写入 ~/.gradle/gradle.properties（机器相关配置，不进仓库）。
python3 - <<'PYEOF'
import os
from urllib.parse import urlparse

p = urlparse(os.environ.get("https_proxy") or os.environ.get("HTTPS_PROXY") or "")
path = os.path.expanduser("~/.gradle/gradle.properties")
lines = []
if os.path.exists(path):
    with open(path) as f:
        lines = [ln for ln in f.read().splitlines()
                 if not ln.startswith("systemProp.http.proxy")
                 and not ln.startswith("systemProp.https.proxy")
                 and not ln.startswith("systemProp.http.nonProxyHosts")
                 and not ln.startswith("systemProp.https.nonProxyHosts")
                 and not ln.startswith("org.gradle.daemon")
                 and not ln.startswith("# Android 代理（android-env.sh 生成）")]
if p.hostname:
    lines.append("# Android 代理（android-env.sh 生成）")
    lines.append(f"systemProp.http.proxyHost={p.hostname}")
    lines.append(f"systemProp.https.proxyHost={p.hostname}")
    if p.port:
        lines.append(f"systemProp.http.proxyPort={p.port}")
        lines.append(f"systemProp.https.proxyPort={p.port}")
    np = os.environ.get("no_proxy") or os.environ.get("NO_PROXY") or ""
    if np:
        lines.append("systemProp.http.nonProxyHosts=" + "|".join(
            x.strip() for x in np.split(",") if x.strip()))
        lines.append("systemProp.https.nonProxyHosts=" + "|".join(
            x.strip() for x in np.split(",") if x.strip()))
    # 本机 daemon 注册表不可靠（daemon 启动后健康检查找不到自己而自停），
    # 直接禁用 daemon，构建在客户端 JVM 内跑完。
    lines.append("org.gradle.daemon=false")
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w") as f:
        f.write("\n".join(lines) + "\n")
PYEOF
