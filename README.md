# Todo 多端应用（第 1 轮：M0–M4，单机可用）

客户端：Tauri 2 · React 19 · TypeScript · Tailwind 4 · Zustand 5 · SQLite（rusqlite）
服务端与同步：第 3 轮（M5/M7–M9）已交付，见下。

## 开发

```bash
pnpm install
pnpm dev            # 浏览器开发（localStorage 单机实现，可记待办）
pnpm tauri dev      # 桌面窗口（需 Linux webkit2gtk 等系统依赖）
pnpm build          # tsc + vite 构建
```

Rust：

```bash
cargo test -p todo-core        # LWW 单测 + repo 单测
cargo build -p todo-app        # Tauri 壳
cargo clippy -- -D warnings
```

## 结构

- `crates/core/`：无 Tauri 依赖。`models.rs`（同步协议唯一定义）、`db/`（schema + repo 统一写入口）、`lww.rs`（纯函数 LWW）。
- `src-tauri/src/`：`commands.rs`（§7 IPC）、`secret.rs`（SecretStore）、`local_notify.rs`（前台到期提醒）、`sync_client.rs`（M5 占位）。
- `src/`：React 前端，移动优先（<768px 单列 + 底部导航 + 抽屉，≥768px 侧边栏）。

## Android（第 2 轮 M6，进行中）

环境脚本：`source android-env.sh`（导出 JAVA_HOME、ANDROID_HOME、ANDROID_SDK_ROOT、NDK_HOME、PATH，并写 `~/.gradle/gradle.properties` 代理与 `org.gradle.daemon=false`）。

```bash
source android-env.sh
export ANDROID_NDK_HOME="$NDK_HOME"
export GRADLE_OPTS="-Xmx2048m -Dfile.encoding=UTF-8 -javaagent:$HOME/.gradle/wrapper/dists/gradle-9.6.1-bin/4ticwg1pgcbps2hj28r8so764/gradle-9.6.1/lib/agents/gradle-instrumentation-agent-9.6.1.jar"
pnpm tauri android build --apk -d -t aarch64 --ci   # debug APK（单 ABI 先行）
```

平台差异（已实现，见 PROGRESS.md M6）：数据库走 `app.path().app_data_dir()`（Android 私有目录）；令牌走 AndroidKeyStore + AES-GCM（`TokenStore.kt`，只写不读）；Android 13+ 首次启动申请 `POST_NOTIFICATIONS`（`MainActivity.kt` + Manifest 声明）；同步指示位保持单机语义。

阻塞（2026-11-11）：沙箱拦截 Java 进程的 TCP，Gradle 下载 AGP/Kotlin/AndroidX 失败（`Garbage in response: muse: Other TCP connections is turned off … switch other_tcp from Deny to Ask`）。需在 Muse 设置 → Permissions → Direct network protocols 把 `other_tcp` 从 Deny 改为 Ask 后再跑上面命令；curl/Python 的联网不受影响。

- 单机模式：无服务端时 CRUD、搜索、拖拽、提醒全部可用；状态区显示"本地修改 N 条待同步"。
- 设置页"测试连接"调 `GET /api/v1/ping`；服务端未上线时友好报错，不阻塞使用。
- 同步引擎（M5）、Android 打包（M6）、服务端（M7–M9）见 `SPEC.md` §18 分期。

## 服务端（第 3 轮 M7–M9，已交付）

```bash
cargo build --release -p todo-server
./target/release/todo-server   # 在项目目录下运行，自动读 .env；默认端口 8787
```

- 当前运行状态（第 4 轮 M12）：服务端由 systemd 管理（`deploy/todo-server.service` 已安装到 `/etc/systemd/system/` 并 enable；`systemctl status/start/stop/restart todo-server`，日志 `journalctl -u todo-server -f`）。开发调试仍可用上面 `./target/release/todo-server` 裸跑，但不要与 systemd 实例同时跑（会抢 8787 端口）。VM 重启后 systemd 会自动拉起服务（/etc 持久性未知，若 unit 丢失从仓库 `deploy/` 正本重装）。
- `.env`（0600，已进 .gitignore，密钥值只存这里）：`TODO_MASTER_KEY`（32 字节 base64，缺失拒启动）、`TODO_ADMIN_PASSWORD`（首次种子 Admin 账号 admin）、`TODO_SYNC_TOKEN`（首次种子同步令牌）、`TODO_PORT`（默认 8787）、`TODO_DB_PATH`（默认 `/data/app.db`，开发用项目内 `data/server.db`）、`TODO_INSECURE_HTTP=1`（开发裸 HTTP；生产走 TLS 反代）、`TODO_POLL_INTERVAL`（提醒 tick 秒数，默认 60）。
- 接口：`GET /healthz`（公开）、`GET /api/v1/ping` 与 `POST /api/v1/sync`（Bearer 同步令牌）、`POST /api/login`、`/api/logout`、`GET/PUT /api/notify`、`POST /api/test/mail`、`/api/test/webhook`、`GET /api/devices`、`/api/conflicts`、`/api/runs`、`/api/ledger`（Admin 会话鉴权）。
- 客户端同步：设置页填服务端地址 + 同步令牌后，启动时、变更后 5 分钟防抖、前台每 15 分钟自动同步，状态区点按立即同步；401 显示"令牌无效"并停自动同步，429 静默退避，离线静默失败、dirty 累积后自动补传。
- 提醒：服务端每 60s 读权威库，到期前按偏移发邮件/Webhook（HMAC 签名），台账幂等、重启不重发，错过 120 分钟内补发。新增依赖（服务端必需）：axum 0.7、tokio、lettre、argon2、aes-gcm、chrono、chrono-tz、hmac、sha2、base64、rand；客户端未新增依赖。详情与验证结果见 `PROGRESS.md`。

## 部署（第 4 轮 M11）

### Docker 构建与启动

```bash
docker build -t todo-server:latest .
# 本机构建环境有出口代理时，构建期透传代理（只用于 cargo 拉取 crates，
# 不进 runtime 镜像；不要在构建参数里放任何密钥）：
docker build -t todo-server:latest \
  --build-arg http_proxy="$http_proxy" \
  --build-arg https_proxy="$https_proxy" \
  --build-arg no_proxy="$no_proxy" .

# compose 启动（先在项目目录备好 .env，见下表；不要把密钥写进 compose 文件）：
docker compose up -d
docker compose ps
curl http://127.0.0.1:8787/healthz
docker compose down        # 停服务，数据卷保留
```

镜像是多阶段构建：builder 用 `rust:1.99-bookworm` 编译 `cargo build --release -p todo-server`，runtime 只剩 `debian:bookworm-slim` + release 二进制（约 8.8MB）+ `ca-certificates`（SMTP/Webhook 走 TLS 时校验证书用）。镜像内不含 `.env` 与任何密钥；`.dockerignore` 已排除 `target/`（本机 13G）、`node_modules/`、`data/`、`.env` 等，不进构建上下文。

### 环境变量（SPEC §14）

| 变量 | 必填 | 默认 | 说明 |
|---|---|---|---|
| `TODO_MASTER_KEY` | ✅ | — | 32 字节 base64；缺失拒启动。只放 `.env` 或注入环境，绝不进文档与镜像 |
| `TODO_ADMIN_PASSWORD` | 首次 | — | 首次启动种子 Admin 账号 `admin` 的密码；之后改密码走 Admin 页 |
| `TODO_SYNC_TOKEN` | 首次（或 Admin 页生成） | — | 首次启动种子同步令牌；之后可在 Admin 页轮换 |
| `TODO_DB_PATH` | | `/data/app.db` | 容器内默认；开发裸跑时用项目内 `data/server.db` |
| `TODO_PORT` | | `8787` | 监听端口 |
| `TODO_POLL_INTERVAL` | | `60` 秒 | 提醒 tick 间隔 |
| `TODO_INSECURE_HTTP` | | —（无默认值，须显式为 `1` 才允许裸 HTTP） | 开发裸 HTTP 用 `1`；生产走 TLS 反代 |

`docker-compose.yml` 里 `TODO_MASTER_KEY` 用 `${TODO_MASTER_KEY:?必须设置}` 做必填校验，缺失时直接报错不启动；其余两项首次种子值为空即跳过种子（已有库不受影响）。

### 备份与恢复

备份 = 复制数据库文件（含 WAL）+ 保存主密钥，两样缺一不可（库是 `TODO_MASTER_KEY` 加密的，丢密钥 = 丢数据）：

```bash
# compose / named volume 部署：
docker compose stop todo-server        # 先停服务，保证 WAL 落盘、文件一致
docker run --rm -v todo-data:/data -v "$PWD/backup:/backup" \
  debian:bookworm-slim cp /data/app.db /data/app.db-wal /data/app.db-shm /backup/ 2>/dev/null
# 再把 .env（或至少 TODO_MASTER_KEY）另外抄一份存到安全处；不要和备份放同一位置
docker compose start todo-server

# 恢复：先停服务，再把备份拷回卷里（覆盖同名文件），最后启动：
docker compose stop todo-server
docker run --rm -v todo-data:/data -v "$PWD/backup:/backup" \
  debian:bookworm-slim cp /backup/app.db /data/app.db
docker compose start todo-server
```

裸跑（systemd / nohup）部署：库在 `TODO_DB_PATH` 指向的位置（开发是项目内 `data/server.db`），停服务后直接 `cp` 该文件及同目录的 `-wal`/`-shm` 即可；恢复同样先停服务再拷回。注意：正在跑的服务不要直接拷库——WAL 没 checkpoint 时拷出来的是不一致快照。

### 反代（生产必须 TLS）

服务本身只监听裸 HTTP（容器内 `TODO_INSECURE_HTTP=1`），生产必须在前面挂 TLS 反代，不要把 8787 直接暴露到公网。Caddy 示例（SPEC §14）：

```caddy
remind.example.com {
    reverse_proxy 127.0.0.1:8787
}
```

把 `remind.example.com` 换成自己的域名，Caddy 会自动签发与续期证书。nginx 同等写法要点：`server { listen 443 ssl; server_name remind.example.com; location / { proxy_pass http://127.0.0.1:8787; proxy_set_header Host $host; proxy_set_header X-Forwarded-Proto $scheme; } }`，证书用 certbot 配好再起服务；只开 443，`TODO_INSECURE_HTTP=1` 仅限开发裸 HTTP 场景使用。

### 保活方案（二选一，不要同时用）

1. **systemd（裸跑部署推荐）**：仓库正本 `deploy/todo-server.service`，安装：
   ```bash
   sudo cp deploy/todo-server.service /etc/systemd/system/todo-server.service
   sudo systemctl daemon-reload
   sudo systemctl enable --now todo-server
   sudo systemctl status todo-server
   ```
   `Restart=on-failure` + `RestartSec=5`，崩溃自动重起；`EnvironmentFile` 读项目目录 `.env`（0600、root 所有，unit 以 `User=root` 运行才读得到）。注意：本机 VM 重启会回滚 `/tmp` 与 apt 装的系统包，`/etc` 的持久性未知——`/etc/systemd/system` 下的只是安装副本，重启后如丢失，从仓库正本按上面三步重装即可。
2. **compose**：`docker-compose.yml` 已设 `restart: unless-stopped`，宿主机重启后 Docker 会自动把容器拉起来（前提是 Docker 守护进程本身随系统启动；本机 dockerd 目前是手动起的，见 PROGRESS.md 环境说明）。

当前开发环境是 `nohup ./target/release/todo-server` 起的服务，VM 重启后不会自动恢复，需手动再起，或改用上面两种保活方案之一。两种方案不要混用：不要同时用 unit 与 compose 起两个实例监听 8787，也不要与开发 nohup 服务同时跑，会端口冲突。

### 本机环境说明（仅适用当前沙箱 VM，正常宿主机忽略）

- Docker 是后装的（`docker.io 29.1.3` + buildx + compose v5.7.0），VM 重启会回滚 apt 包，重启后需等平台 OS-intent 回放结束再检查/重装；dockerd 须手动起，命令与参数见 `PROGRESS.md` 环境一节。
- **本机目前无法运行容器**（沙箱禁 netlink/setns/keyctl 等，`docker build`/`docker run` 均在 runc init 处 EPERM）：镜像构建与容器运行须换宿主机，`Dockerfile`/`docker-compose.yml` 本身已校验通过（`docker buildx build --check` 无警告、`docker compose config -q` 通过）。
- `/tmp` 是 512M tmpfs，不要把镜像、大缓存放里面（曾写满导致服务重启失败）；大文件放 `~/workspace/scratch/`。
