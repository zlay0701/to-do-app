# Todo 多端应用 · 进度（PROGRESS.md）

> 唯一事实来源：`SPEC.md`（v4.4，2026-11-11）。本文件只记完成 / 遗留 / 下一步（SPEC §16 第 10 条）。

## M0 骨架 — 完成
- 完成：Cargo workspace（crates/core + src-tauri + 前端）；core 无 Tauri 依赖；React 19 + TS 5 + Tailwind 4 + Zustand 5，不引 UI 库；`cargo build -p todo-app`、`cargo test`、`cargo clippy -- -D warnings`、`cargo fmt --check`、`pnpm build`、`pnpm dev`、`pnpm tauri dev` 全部通过。
- 遗留：无（环境问题已解决，见"环境"）。
- 下一步：M6 直接进 Android 构建链（JDK 17 / NDK r26 / cargo-ndk）。

## M1 core 数据层 — 完成
- 完成：四表 schema（§3 原样）、repo.rs 统一写入口（写必打 updated_at=now、dirty=1、origin=deviceId）、uuid v4、软删除 tombstone、首次启动"收件箱"（dirty=1）；lww.rs 纯函数零 IO，单测覆盖新胜旧、旧不胜新、同时间同内容、同时间异内容（origin 大者胜）、tombstone 复活/删除；同步协议结构体只在 core/models.rs 定义一次；11 个单测全绿。
- 遗留：无。
- 下一步：M5 用 repo.dirty_changes() + lww.decide_* 接同步（第 3 轮）。

## M2 客户端主界面 — 完成
- 完成：移动优先布局（<768px 单列 + 底部导航 + 抽屉，≥768px 侧边栏）；列表/任务全链路 CRUD、计数角标、同步指示位（单机显示"本地修改 N 条待同步"）；桌面端实测建库：~/.local/share/com.example.todo/todo.db 含收件箱（dirty=1）与 device_id；浏览器 dev 用 localStorage 同语义实现，可记待办。
- 遗留：浏览器里未逐项点过全链路（只验证了服务与构建）；桌面端未逐项点过 UI（只验证了启动与建库）。
- 下一步：第 2 轮真机/桌面实测时逐项点一遍；列表拖拽排序未做（仅任务拖拽）。

## M3 客户端交互 — 完成
- 完成：搜索（标题/备注）、拖拽排序（中点 sortOrder）、已完成折叠、空状态、快捷键（N 新建聚焦、/ 搜索聚焦、Esc 关闭/返回）。
- 遗留：同 M2，待实测点一遍。
- 下一步：同 M2。

## M4 客户端设置 + 本地通知 — 完成
- 完成：设置页服务端分区（URL + 同步令牌 + 测试连接 GET /api/v1/ping + 自动同步间隔）与本地通知分区（启用 + 提前量）；SecretStore：keyring 优先，Linux 无 keyring 守护进程时回退 0600 文件（记警告，Windows 不受影响），令牌只写不读（返回 hasToken）；前台到期提醒：Rust due_notifications + 前端 30s 轮询横幅 + 可选系统 Notification 授权；sync_now 含 BUSY 互斥，未配服务端时报未配置（真同步在 M5）。
- 遗留：桌面端 SecretStore keyring 与提醒横幅未在真机上点过（只验证了代码与编译）；服务端未上线，测试连接现为友好报错。
- 下一步：第 3 轮接 M5/M7–M9 时再联调测试连接与同步。

## M6 Android 适配 — 进行中（阻塞未解除：Gradle 仍被 other_tcp 拦截；桌面构建已恢复）
- 完成：构建链装好（见"环境"）；`pnpm tauri android init` 生成 `src-tauri/gen/android/`；compileSdk/targetSdk 37→36（公开仓库无 android-37）；Rust aarch64 编译通过（`cargo ndk check/clippy -p todo-app` 无告警）；业务代码平台差异已写好：数据库走 `app.path().app_data_dir()`（Android 私有目录，无需改）；SecretStore Android 侧用 AndroidKeyStore + AES-GCM（新建 `TokenStore.kt` 密钥别名 `todo-sync-token`、新建 `android_secret.rs` 经 JNI 调 Kotlin 助手，令牌只写不读，`jni = "0.21"` 已是 Tauri 传递依赖、提升为直接依赖）；MainActivity 首次启动申请 `POST_NOTIFICATIONS`（Android 13+）+ Manifest 声明；TokenStore.kt、MainActivity.kt 已用 kotlinc 2.2.10 对 android-36 编译通过；Gradle 已打通进程内构建（沙箱禁 daemon 进程间 TCP，见阻塞）。2026-11-11 13:25 续跑：桌面 apt 依赖已从本地缓存全部恢复（见"环境"），`cargo build -p todo-app` 与 `cargo clippy --all-targets -- -D warnings` 均通过。
- 遗留：APK 尚未产出，卡在 Gradle 配置阶段：buildSrc 解析 `kotlin-dsl` 插件 6.6.4 时下载失败。2026-11-11 13:25 续跑实测阻塞未解除：Java 直连探针 `Socket.connect(plugins.gradle.org:443)` 返回 TCP-OK（只证明到出口代理的 TCP 通），但 `--debug` 再确认代理原文 `Garbage in response: muse: Other TCP connections is turned off for this assistant. To allow it, ask the user to open Muse settings -> Permissions -> Direct network protocols and switch other_tcp from Deny to Ask`；直连（--noproxy）SSL wrong version number、走代理 curl 200——说明出口只允许经代理的 curl/Python，Java HttpClient 仍被拦截。恢复办法不变：用户在 Muse 设置里把 `other_tcp` 从 Deny 改为 Ask 后重跑 `source android-env.sh` + `export ANDROID_NDK_HOME="$NDK_HOME"` + GRADLE_OPTS（见 README.md Android 一节）+ `pnpm tauri android build --apk -d -t aarch64 --ci`；列表拖拽排序未做（记遗留）；图标仍为占位图；未在真机/模拟器安装验证（无 APK，未声称已验证）。
- 2026-11-11 第 3 轮开场重测（日志 /tmp/m6-retest-r3.log，未再重试）：`pnpm tauri android build --apk -d -t aarch64 --ci` EXIT=1；pnpm build 与 Rust aarch64 编译通过，Gradle 配置阶段同一失败——buildSrc 解析 kotlin-dsl 插件 6.6.4 找不到（BUILD FAILED in 13s），无 APK。结论：阻塞未解除，直接做服务端（M7–M9/M5），不再重试、不手动种子依赖。
- 2026-11-11 第 4 轮开场重测（日志 logs/m6-retest-r4.log，只跑一次）：同一命令 EXIT=1，Gradle 配置阶段同一失败——kotlin-dsl 插件 6.6.4 找不到，无 APK；直接 `./gradlew help --debug` 本次在 daemon 分发处失败（沙箱禁 daemon 进程间 TCP），未再捕获代理原文，前两轮 --debug 已确认根因为 other_tcp 拦截。结论不变，不再重试。
- 2026-11-11 第 5 次重测（用户要求再试一次；日志 logs/m6-retest-r5.log，只跑一次）：同一命令 BUILD_EXIT=1；pnpm build 通过（dist JS 256.53 kB）、Rust aarch64 编译通过（23.18s）、.so 已 symlink 到 jniLibs/arm64-v8a；Gradle 配置阶段同一失败——buildSrc 解析 kotlin-dsl 插件 6.6.4 找不到（`could not resolve plugin artifact 'org.gradle.kotlin.kotlin-dsl:org.gradle.kotlin.kotlin-dsl.gradle.plugin:6.6.4'`，BUILD FAILED in 13s），无 APK。本次未再跑 --debug（任务要求只跑一次构建），本次日志本身未再捕获代理原文；但失败签名（同一插件、同一阶段、同一 13s）与前四次完全一致，前两轮 --debug 已确认根因为 other_tcp 拦截。结论不变，不再重试。
- 2026-11-11 第 6 次重测（用户说“好了”要求再试；日志 logs/m6-retest-r6.log，只跑一次）：本机同一命令 BUILD_EXIT=1；pnpm build 通过（dist JS 256.53 kB）、Rust aarch64 编译通过（14.09s）；Gradle 配置阶段同一失败——buildSrc 解析 kotlin-dsl 插件 6.6.4 找不到（BUILD FAILED in 48s），无 APK。随后按任务顺序探 SSH 第二条路（<remote-host>，ssh -p <ssh-port>）：出口代理直接重置连接（`kex_exchange_identification: read: Connection reset by peer / Connection reset by <proxy-ip> port <proxy-port>`），未连上；且该 SSH 操作被拒绝，不再重试、不换绕路方案。结论：两条路均仍被 other_tcp 拦截，须用户把 other_tcp 从 Deny 改为 Ask 后再试。
- 2026-11-11 第 7 次重测（用户要求按之前的方式本机再试、不用远程机器；日志 logs/m6-retest-r7.log，只跑一次）：同一命令 BUILD_EXIT=1；pnpm build 通过（dist JS 256.53 kB）、Rust aarch64 编译通过（12.15s）；Gradle 配置阶段同一失败——buildSrc 解析 kotlin-dsl 插件 6.6.4 找不到（`could not resolve plugin artifact 'org.gradle.kotlin.kotlin-dsl:org.gradle.kotlin.kotlin-dsl.gradle.plugin:6.6.4'`，BUILD FAILED in 16s），无 APK。本次未再跑 --debug（任务要求只跑一次构建），本次日志本身未再捕获代理原文；但失败签名（同一插件、同一阶段）与前六次完全一致，前两轮 --debug 已确认根因为 other_tcp 拦截。本次未做任何 SSH 或对外直连。结论不变，不再重试。
- 2026-11-11 第 8 次重测（用户说权限已打开、other_tcp 改为 Ask，需要联网审批时问用户；日志 logs/m6-retest-r8.log，只跑一次本机构建）：同一命令 BUILD_EXIT=1；pnpm build 通过（dist JS 256.53 kB）、Rust aarch64 编译通过（11.88s）；Gradle 配置阶段同一失败——buildSrc 解析 kotlin-dsl 插件 6.6.4 找不到（`could not resolve plugin artifact 'org.gradle.kotlin.kotlin-dsl:org.gradle.kotlin-dsl.gradle.plugin:6.6.4'`，BUILD FAILED in 13s），无 APK。构建全程未出现联网审批等待（13 秒即失败，未触发审批卡片）；本次未再跑 --debug（任务要求只跑一次构建），本次日志本身未再捕获代理原文；但失败签名（同一插件、同一阶段、同一 13s）与前七次完全一致，前两轮 --debug 已确认根因为 other_tcp 拦截。结论：other_tcp 仍未实际放行（设置已改但行为未变，或 Ask 模式下该类非交互连接未触发审批即被拒），不再重试。
- 下一步：other_tcp 放行后跑通 debug APK（核对 APK 路径/大小 ≤15MB、包名 com.example.todo、version 0.1.0），再试 release 签名；有模拟器/真机时装机验证建库（收件箱 dirty=1）与一次 CRUD，否则明确写"未在真机安装验证"；再跑四 ABI 构建。

## M7 服务端骨架 — 完成
- 完成：workspace 增加 `crates/server`（binary `todo-server`，axum 0.7 + tokio + rusqlite WAL 单写者）；服务端 schema 按 §4 原样（lists/tasks 含 server_rev、meta rev_counter、devices、conflict_log、reminder_ledger、runs、admin、secrets，另加 config 表存通知配置 JSON）；TODO_MASTER_KEY（32 字节 base64）缺失拒启动，裸 HTTP 须 TODO_INSECURE_HTTP=1 否则拒启动；同步令牌/SMTP 密码/Webhook 密钥 AES-256-GCM 加密存 secrets 表（nonce 随机）；Admin 单账号 argon2id，POST /api/login、/api/logout 先行（session HttpOnly + SameSite=Lax + Secure，24h，内存表）；GET /healthz（{ok, lastRunAt, lagSeconds}）；`cargo build --release -p todo-server` 通过，二进制 8.8MB（≤25MB）。
- 遗留：Admin session 存内存，服务端重启后需重新登录（M10 可改为落库）；无 TLS（本轮开发用裸 HTTP + TODO_INSECURE_HTTP=1，生产走反代，见 §14）。
- 下一步：第 4 轮做 M10 Admin 页、M11 部署（Dockerfile/compose/反代）。

## M8 服务端同步中枢 — 完成
- 完成：POST /api/v1/sync（Bearer 令牌常量时间比较，401 不区分令牌不存在/不匹配；每设备最小间隔 5 秒，违反 429 TOO_FREQUENT；GET /api/v1/ping 供测试连接）；请求/响应字段严格按 §9.1；合并严格按 §9.2 用 core/lww.rs 的 decide_*（不存在→插入；新胜旧；旧被拒记 conflict+1 写 conflict_log；同时间异内容 origin 大者胜、败方记 conflict_log；tombstone 同规则；每次写入事务内 bump rev_counter→写记录→提交，整轮合并一个事务）；拉取按 server_rev > cursor，单次最多 5000 条，cursor 取本轮实际最大 rev（截断不丢记录）；clockOffsetMs = clientTime - serverTime，|offset| > 120s 时 devices.clock_offset_ms 记录。
- 遗留：无（curl 覆盖见"验证记录"）。
- 下一步：第 4 轮做 M12 端到端验收（服务端清库重建、客户端 cursor 归零等场景）。

## M9 服务端提醒 — 完成
- 完成：每 60s tick 直查权威库（status=0 AND due_at IS NOT NULL AND deleted_at IS NULL），候选点 due_at - offset，命中窗口 [now-tick, now+tick]，错过的在 catch_up_min（默认 120 分钟）内补发一次、超出丢弃；reminder_ledger 唯一键（task_id, kind, anchor, offset_min, channel）INSERT OR IGNORE，行数=1 才发送（重启不重发）；改期→新 anchor→再提醒；完成后不提醒；邮件模板 [待办] <标题>（摘要 [待办] 今日 N 项待办），纯文本 + HTML，不含笔记全文；Webhook 头 X-Todo-Event 与 X-Todo-Signature sha256=<hmac>，2xx 成功，4xx（除 408/429）不重试，5xx/超时重试 3 次（30s/2min/10min）；摘要时区用 notify_config 的时区（chrono-tz）；配置接口 GET/PUT /api/notify（Admin 鉴权）与 POST /api/test/mail、/api/test/webhook、GET /api/devices、/api/conflicts、/api/runs、/api/ledger 全部落齐（Admin 页本身是 M10，本轮只做接口）。
- 遗留：SMTP 无真实账号未实测发信（/api/test/mail 无配置时 400 友好报错已验证；发信失败台账 failed + 友好错误"用授权码而非登录密码"/"建议 587 + STARTTLS"已实现，未实测）；每日摘要逻辑已实现但未单独实测触发（digest_enabled 默认 false）。
- 下一步：有真实 SMTP 账号时实测一封邮件；第 4 轮在 Admin 页接测试发送按钮。

## M10 Admin 页 — 完成
- 完成：内嵌单页 `crates/server/src/admin.html`（HTML + 原生 JS，`include_str!` 编进二进制，零新依赖），路由 GET /admin；三 tab——连接（/healthz、/api/devices 含游标与时钟偏差，|offset|>120s 标黄、同步令牌生成/重置）、通知（GET/PUT /api/notify、POST /api/test/mail、/api/test/webhook）、运行（/api/runs、/api/ledger、/api/conflicts）；会话改落库（新增 sessions 表 + repo 函数，登录写库、重启后旧 Cookie 仍有效，第 3 轮内存表遗留已消除）；新增 GET/POST /api/token（令牌状态查询与生成/重置，新令牌只返回一次，旧令牌即刻失效）。
- 遗留：无真机/无头浏览器——Admin 页未在真实浏览器里点过（curl + node --check + cookie 流程已验证，见验证记录）；Secure Cookie 须经 localhost 或 HTTPS 访问（SPEC §11 要求，LAN IP 裸 HTTP 下浏览器不发 Secure Cookie）。
- 下一步：第 4 轮 M12 验收时在 Admin 页当场查看冲突日志与台账。（已完成：M12 当场查过，ledger 8 行、conflicts 3 行、runs 21 行均可见，见验证记录。）

## M5 客户端同步引擎 — 完成
- 完成：src-tauri/src/sync_client.rs 接通（BUSY 互斥保留；reqwest 30s 超时 POST /api/v1/sync，401/429/网络分类错误）；commands.rs sync_now 改为 async：收集 dirty → POST → Repo::apply_sync 单事务应用（§9.3：已推记录仅当本地 updated_at ≤ 推送值才清 dirty，往返期间新改动保留；apply 本地 dirty 且更新则保留本地，否则服务端权威态覆盖并清 dirty；写 cursor/clock_offset_ms/sync_client_state；apply 满 5000 条时空 changes 循环拉取）；core 新增 Repo::apply_sync + 事务内 upsert（服务端态原样写入，不打新戳）及 3 个单测；401 停止自动同步（auto_sync 置 0）+ 状态区红色"令牌无效"，429 静默退避，超时/失败不调 apply_sync（dirty 保留）；时钟漂移 |offset| > 120s 时 SyncBadge 黄色提示；触发：手动"立即同步"（SyncBadge 点按）+ 启动时 + 变更后 5 分钟防抖 + 前台每 15 分钟（前端定时，见 App.tsx/store.ts）；离线静默失败、dirty 累积。
- 遗留：直接跑 `target/debug/todo-app` 二进制时前端未加载（未触发同步），`pnpm tauri dev` 路径已验证完整往返——直接二进制的产物嵌入问题待第 4 轮用 `pnpm tauri build` 核查（第 4 轮已核查：`pnpm tauri build` 通过，产出 deb 4.73MiB / rpm 4.73MiB / AppImage 79.6MiB；直接跑 release 二进制 `target/release/todo-app`（xvfb）前端正常加载，启动同步成功 cursor=13，见 M12 验证记录；debug 二进制直跑未再单独复测）；另修掉一个第 1 轮遗留 bug：TauriApi 里 `list_tasks`/`create_task`/`reorder_list`/`reorder_task` 的 invoke 参数用了 snake_case（`list_id` 等），Tauri 2 要求 camelCase（`listId`），导致 refreshAll 在 listTasks 处被拒、settings 永不加载——已全改为 camelCase；另修 SecretStoreImpl.get_token：keyring 返回 NoEntry（Ok(None)）时也应查回退文件，否则文件里的令牌永远读不到（keyring 探针实测确认）。
- 下一步：第 4 轮核查直接二进制/打包后的同步；M12 验收时测离线补传与 401/429 界面态。

## M11 部署 — 完成（容器实测未通过，见遗留）
- 完成：`Dockerfile`（多阶段：`rust:1.99-bookworm` 构建 `cargo build --release -p todo-server` + `debian:bookworm-slim` 运行，基础镜像可用 `RUST_IMAGE`/`RUNTIME_IMAGE` 构建参数覆盖；代理只作构建参数透传，不进镜像层；镜像内不含 `.env` 与任何密钥；`TODO_DB_PATH=/data/app.db`、`TODO_PORT=8787`、`VOLUME /data`）、`docker-compose.yml`（named volume `todo-data` 挂 `/data`、`8787:8787`、`restart: unless-stopped`、`TODO_MASTER_KEY` 用 `${TODO_MASTER_KEY:?…}` 必填校验）、`deploy/todo-server.service`（仓库正本：`User=root` + `EnvironmentFile` 读项目 `.env`，`Restart=on-failure` + `RestartSec=5`）、`.dockerignore`（排除 `target/` 等大目录）、README「部署」一节（构建/compose 命令、环境变量表、备份恢复、反代、保活二选一）。
- 遗留：本机沙箱无法运行容器，`docker build` 未通过、容器 `/healthz` 测试与 `compose up` 均未做——失败点在首个 RUN（runc init EPERM），不是 Dockerfile 本身（`docker buildx build --check` 通过，见验证记录）；镜像构建与容器运行须在正常宿主机（或沙箱放开 netlink/setns/keyctl 后）重做；`todo-server` 在 07:2x 运行时重启后未自动恢复，需父代理重启（本轮按要求未碰服务）。（M12 已解决：服务已重启并改走 systemd 管理，见 M12 验证记录。）
- 下一步：M12 验收前在正常宿主机上跑通 `docker build` + 容器 `/healthz` + `compose up`。

## M12 端到端验收 — 完成（修复轮次后全部通过，见 §17 清单与修复轮次）
- 完成：同步语义 22/22（首次全量、增量只拉新增、LWW 双向顺序、同时同内容 origin 大者胜、tombstone、时钟漂移 +300000ms、cursor=0 全量重推收敛、401/429）；提醒场景全过（到期只发一次、改期再提醒、完成不提醒、90 分钟补发、5 小时丢弃、重启不重发、mail 失败台账 failed 且不重试、每日摘要一次、Admin 台账/冲突/运行均可见）；服务端清库重建协议层通过（rev 0→12，全量拉回一致）；桌面端真实同步（启动同步、断网 dirty 累积保留、恢复后自动补传、cursor 置 0 后全量恢复）；列表拖拽回归（`pnpm build` 通过，`cargo test -p todo-core` 15 passed）；systemd 保活 live 测试通过（install + enable + start + restart，服务现由 systemd 管理）。
- 遗留：**服务端清库重建后真实客户端旧 cursor 静默丢数据（M12 当时未通过，修复轮次已修，见下节）**——客户端无清库检测，cursor=2 对重建后服务端拉到 10 条但因所属列表 L1 未拉到而全部跳过，cursor 却推进到 12、状态显示 ok，后续同步永不补回；须手动把客户端 cursor 置 0 才能恢复，应用内无重置入口（见 §17 清单与验证记录）；Admin 页未在真实浏览器里点过（M10 遗留延续）；窄屏只验证到构建产物含断点、未渲染实测；SMTP 成功路径未用真实邮箱实测（本地自签证书被 lettre 拒，失败路径已验证）；Webhook 5xx 重试 3 次的定时未实测（须 12 分钟以上）；UI 未逐项人工点过；图标仍为占位图；APK 未产出（M6 阻塞未解除）。
- 下一步：修复轮次已完成（见下节）；正常宿主机上跑通 `docker build` + 容器 `/healthz` + `compose up`；真实邮箱实测 SMTP 成功路径；浏览器实测 Admin 页与窄屏。

### §17 验收清单逐项结果（2026-11-11，均实测，见验证记录）
- 客户端断网 CRUD 与 dirty 累积：通过（桌面端实测：服务端停机时同步失败记 error，dirty=1 保留不丢）。
- 恢复网络自动补传：通过（服务端恢复后重启客户端，启动同步把离线任务推上服务端，dirty 清 0）。
- 首次同步全量拉取：通过（新设备 cursor=0 拉到全部记录）。
- 增量只传 dirty：通过（服务端：增量只拉到新增 1 条；客户端：只收集 dirty 推送，见 commands.rs `dirty_changes`）。
- 删除传播（tombstone）：通过（B 拉到 `deletedAt` 非空）。
- 窄屏布局：部分通过（源码移动优先 + 构建产物 CSS 含 `@media(min-width:48rem`；未在真机/浏览器渲染中逐项点过）。
- A 12:00 改标题 X、B 12:05 改同一条 Y → 双方均为 Y：通过。
- 反向顺序（B 先同步）：**与 §17 字面不符**——§17 第二行写“双方均为 X，B 端推的旧值被服务端拒绝且计 conflict”，实测不变量是新 `updated_at` 者胜：B 先推新 Y 落地，A 后推旧 X 被拒、conflicts=1、conflict_log 胜 B/败 A，双方均为 Y。§9.2（新者胜）与实测一致，§17 该行措辞疑似笔误，如实记录，未硬凑。
- `updated_at` 相同内容不同 → origin 大者胜：通过（双向验证：m<z 被拒、z>m 胜出，conflict_log 有记录）。
- conflict_log 有记录且 Admin 可见：通过（3 行，`/api/conflicts` 可见）。
- 客户端时钟快 5 分钟 → 状态区黄色告警、Admin 设备页标黄：部分通过（协议层 clockOffsetMs=299999 回传、devices 表已记；SyncBadge 代码含 |offset|>120s 变 amber 的判断；真实渲染中的黄色未亲眼看到，无浏览器驱动）。
- 清空客户端数据重同步 → 全量拉回、dirty 重推收敛无重复无丢失：通过（cursor=0 全量拉回 7 条，重推后 T1 无重复、标题为新值）。
- 服务端清库重建 → 客户端 cursor 归零后数据恢复：**通过（修复轮次，真实客户端）**——见修复轮次验证记录：generation 变化自动触发全量重同步，列表 + 全部任务恢复，本地干净记录也重推回新库，双方收敛；孤儿任务跳过时 cursor 不推进、状态记 error，下轮补回；手动“重置同步”后下次同步全量拉回。M12 当时“未通过（真实客户端）/ 通过（协议层）”已闭合。
- 提醒到期只发一次：通过（第 2 个 tick 不重发，台账 webhook status=1 attempts=1）。
- 提醒重启不重发：通过（两次重启后 R1=2 次、R4=1 次计数不变，mail attempts 仍为 1）。
- 改期后下一 tick 按新时间提醒：通过（新 anchor 再发一次，台账新增一行）。
- 完成后不再提醒：通过（完成任务 0 事件、0 台账行）。
- 停机 90 分钟重启 → 120 分钟内补发一次：通过（task.overdue 1 次）。
- 停机 5 小时 → 不补发：通过（0 事件、0 台账行）。
- Webhook 签名可验证：通过（接收器 HMAC-SHA256 验签 sig_ok=true，ping 与提醒事件均验证）。
- SMTP 失败不无限重试：通过（失败路径：自签证书被 lettre 拒，台账 mail status=2 attempts=1，多 tick 与重启后 attempts 仍为 1；成功路径未用真实邮箱实测）。
- 每日摘要：通过（digest.daily 触发一次，台账 `__digest__` anchor=20261111 status=1，不重复）。
- 工程：`cargo build --release -p todo-server` 通过（8.8MB）；`cargo test --workspace` 18 passed（core 15 + server 3）；`cargo clippy --all-targets -- -D warnings` 无告警；`cargo fmt --check` 通过；非测试代码无 `unwrap()`；`pnpm build` 通过；`pnpm tauri build` 通过（deb 4.73MiB、rpm 4.73MiB、AppImage 79.6MiB；直接跑 release 二进制前端正常加载并同步）。

## 修复轮次（M12 唯一未通过项，2026-11-11）— 完成
- 修复：服务端清库重建后真实客户端旧 cursor 静默丢数据。方案（纵深防御，两者都做，SPEC v4.4）：（1）服务端 generation 代际标识——库初始化时 `meta key='generation'` 生成 uuid v4，每次 `/api/v1/sync` 响应带上；客户端存进 `sync_client_state.generation`（旧库迁移补列），不一致（或本地为空且 cursor>0）即按首次同步处理：cursor 归零、全量拉取、本地全部记录重新标记 dirty 后重推，LWW 收敛；generation 变化不是错误，状态区不报红，只记日志。（2）`apply_sync` 孤儿任务修正——选方案 (a) 变体：列表先于任务应用，仍缺列表的任务跳过且整轮 cursor 保持同步前的值（apply 不带 server_rev，无法精确停在第一条被跳过记录的 rev 之前，保守做法是整轮不推进，下轮从旧 cursor 重拉，重拉幂等），状态记 error + last_error 写明跳过条数与原因；另加设置页“重置同步”入口（IPC `reset_sync`，确认提示，cursor 与 generation 清零、本地全部记录重新标记 dirty，下次同步全量重拉重推），覆盖 generation 因故未触发的兜底场景。协议变更手续按 SPEC §16 第 8 条 + §0 规则 2：先改 `core/models.rs`（`SyncResponse`/`SyncClientState` 加 `generation`）与 SPEC.md（v4.4：§9.1 响应示例与说明、§9.5 加清库重建行、§4 meta 加 generation、§3 `sync_client_state` 加 generation 列、§7 加 `reset_sync`、§12 孤儿任务行改写、文末变更记录）再动两端代码；§17“反向顺序双方均为 X”那行保持原样（疑似笔误，未经用户定夺不改）。
- 修复中发现并补强：只重推原 dirty 会让新库永久缺失本地已同步过的干净记录（第二台设备也拉不到），与“按首次同步处理 / LWW 自然收敛”不符——改为 generation 变化或手动重置时本地全部记录重新标记 dirty 后重推（tombstone 也一并重推，删除语义不丢；重推与服务端已有记录内容相同时 LWW 判定忽略，不产生多余 rev）。实测佐证：重推把仅存于客户端的 M12 时代任务（R1/R3/R4/R5、T1/T2、TD 等）恢复回了新库，R1 的过期提醒随后在 catch-up 窗口内正常补发一次。
- 完成：`cargo test --workspace` 22 passed（core 19 + server 3，全绿；新增单测：generation 不一致六种组合、孤儿任务不推进 cursor 且记 error、generation 落库、重置后 cursor=0 且全部记录重新变 dirty、`mark_all_dirty`）；`cargo build --release -p todo-server` 与 `-p todo-app` 通过（server 9.2MB、app 12.1MB）；`cargo clippy --workspace --all-targets -- -D warnings` 无告警；`cargo fmt --check` 通过；`pnpm build` 通过（tsc + vite，dist JS 256.53 kB）；非测试代码无 `unwrap()`；无新增依赖（uuid 已在 workspace 内，睡眠用 `tauri::async_runtime::spawn_blocking` 避开客户端对 tokio 的直接依赖）。
- 遗留：只删业务表不删 `meta` 的“软清空”不在本轮范围（generation 不变，仍可能静默丢数据——须走“重置同步”兜底）；“重置同步”按钮本身未在真实浏览器/窗口里点过——沙箱无 xdotool/Xlib 等点击工具，按钮→IPC 的链路只验证到 invoke 注册（编译）+ TS 类型检查（`pnpm build`）+ 单元测试，按钮触发后的行为（cursor=0、generation=NULL、全部 dirty → 下次同步全量拉回）是用与 `reset_sync_state` 完全相同的 SQL 置数后经真实客户端启动同步验证的；§17 反向顺序措辞已于 2026-11-11 经用户确认改为 Y（SPEC v4.5）；其余 M12 遗留不变（Admin 页/窄屏未真实点过、SMTP 成功路径未实测、Webhook 5xx 重试定时未实测、图标占位、APK 未产出）。
- 下一步：M6（other_tcp 放行后跑通 APK）；正常宿主机跑通 Docker 构建与容器测试；真实邮箱实测 SMTP 成功路径；浏览器实测 Admin 页与窄屏；有真机/浏览器驱动时点一遍“重置同步”按钮与设置页。

### 修复轮次验证记录（2026-11-11，均实测）
- 复现（未修复代码，`logs/fix_repro_prep.py` + `pnpm tauri dev` xvfb，日志 `logs/fix-tauri-dev-pre.log`）：服务端清库重建（L9 + 11 条 FX0–FX10，rev 1..12），桌面客户端 cursor=2、无 L9；启动同步 `sync round 0: ok cursor=12 conflicts=0 apply=10`，10 条因 L1 未拉到全部跳过，cursor 却推进到 12、状态 ok；客户端事后 FX 任务 0 条、总任务 13 条，bug 复现无误。备份在 `logs/fix-backup/server.db.fix-pre` 与 `todo-client.db.fix-pre`。
- 修复后重跑同一场景（`logs/fix_verify_post.py`，备份 `*.fix-post`）：第一次（仅 dirty 重推版）：启动同步 round 0 `apply=10` 后日志出现 `[todo] sync: generation 变化，服务端已重建，cursor 归零全量重同步`，round 1 `ok cursor=12 apply=12`；客户端 L9 + 11 条 FX 全部恢复、总任务 24、cursor=12、generation 已存、状态 ok——但服务端仍缺客户端本地的 L1/收件箱记录（干净记录未重推），故补强为全部记录重推。第二次（最终版，先删客户端 L9/FX 还原 M12 初始态）：round 0 `apply=10` → generation 变化 → round 1 `ok cursor=27 apply=27`；客户端 L9 + 11 条 FX 恢复、总任务 24、cursor=27、generation 与服务端一致、状态 ok；服务端同时恢复为 3 列表 + 24 任务、rev_counter=27——双方收敛。
- 手动“重置同步”（真实客户端启动同步验证，触发状态用与 `reset_sync_state` 完全相同的 SQL 置数：cursor=0、generation=NULL、全部 dirty；按钮点击本身未自动化，见遗留）：置旧 cursor=2 并删本地 L9/FX（generation 与服务端一致，不触发 generation 路径）→ 重置 → 重启客户端，启动同步 round 0 `ok cursor=27 apply=27`，L9 + 11 条 FX 全量拉回、cursor=27、generation 重新存下、状态 ok。重推与服务端已有记录内容相同时 LWW 判定忽略（rev_counter 未多涨）。
- 回归 A（`logs/fix_regression.py`，纯 HTTP，9/9 通过）：401 无令牌被拒绝；429 同设备 5 秒内重复同步被限频；LWW 双向（B 更新值胜、A 再更新值胜）；同时间 origin 大者胜（dev-reg-b）；tombstone 服务端落删除标记且全量拉取带 `deletedAt`；增量 cursor=最新 rev 时 apply 为空；首次全量 apply 数等于服务端全量（3 列表 / 27 任务）。
- 回归 B（真实客户端，`pnpm tauri dev` xvfb）：孤儿任务跳过——服务端先推无列表任务 OR1（rev 35），客户端 cursor=0 同步 round 0 `apply=31`，OR1 被跳过未入库，cursor 保持 0、last_result=error、last_error“1 条任务因所属列表未同步而暂存，下次同步重试”，其余 27 条正常入库；随后补推列表 L-orphan（rev 36），重启客户端 round 0 `apply=32`，OR1 与列表一并拉回，cursor=36、状态 ok。离线——服务端停机时启动客户端：同步失败记 error（`无法连接服务端`），离线任务 OFF1 dirty=1 保留、cursor 不动；服务端恢复后重启客户端：启动同步 `ok cursor=37 apply=1`，OFF1 已上服务端，客户端 dirty 清 0、状态 ok。
- 回归 C（提醒抽查，Webhook 接收器 127.0.0.1:8899 + Admin 配置 webhook 通道，日志 `logs/hook-events.jsonl`，旧日志备份为 `hook-events.jsonl.fix-bak`）：到期任务 RM1 只发一次（35 秒内 1 次 task.overdue，台账 webhook status=1 attempts=1；再等 70 秒仍为 1 次）；改期后新 anchor 再发一次（45 秒内第 2 次事件，台账新增一行）；抽查结束已把通知配置改回禁用并停掉接收器。附带：全量重推恢复回新库的 M12 任务 R1（仅存于客户端）在 catch-up 窗口内正常补发一次 task.overdue。
- 工程与线上验证：服务端改完后 `systemctl restart todo-server`，`/healthz` ok；curl 真实同步验证线上为新版本（响应带 `generation`，cursor=39、apply=0）；提醒抽查后通知配置已禁用；当前服务端库状态：4 列表（inbox/L1/L9/L-orphan）、30 任务、rev_counter=39、generation=3160ee5a、ledger 3 行、conflict_log 1 行（回归测试数据，非用户数据）。

## v4.7（SPEC 文字与数字包 + §9.3 方案 A，2026-11-11）— 完成
- 完成（a）文字与数字包：两份文件同步改相同行，项目 SPEC 升 v4.7、原件仍保持 v4.0 范围：§18 轮次预算按 §15 里程碑表求和重定（轮 1 1,550–3,250、轮 2 800–1,600、轮 3 2,800–5,600，两处小计 2,350–4,850 / 5,150–10,450，四轮求和 6,100–12,350 与 §15 合计一致，改后已验算）；§1.3 workspace_id 只改措辞（暂不设列，多工作区以后再加）；§15 删 v3 对比段（LWW 纯函数那行保留）；§9.2 括号措辞改顺；v4.0 变更记录历史数字不动；§9.3 补被拒记录权威版本随 apply 下发一句。
- 完成（b）§9.3 方案 A：服务端 `merge_changes` 被拒时收集权威版本，`sync` 响应把被拒记录的服务端权威版本放进 apply（去重，不因此推进 cursor）；客户端不用改代码（`apply_sync` 先清 dirty 再应用 apply，正好覆盖）。回归测试新增 2 个：服务端 `rejected_record_authoritative_version_in_apply`、客户端 `apply_sync_rejected_record_converges_only_when_server_version_in_apply`。
- 遗留：只删业务表不删 `meta` 的“软清空”不在本轮范围（generation 不变，仍可能静默丢数据——须走“重置同步”兜底）；“重置同步”按钮本身未在真实浏览器/窗口里点过；其余 M12 遗留不变（Admin 页/窄屏未真实点过、SMTP 成功路径未实测、Webhook 5xx 重试定时未实测、图标占位、APK 未产出）。
- 下一步：M6（other_tcp 放行后跑通 APK）；正常宿主机跑通 Docker 构建与容器测试；真实邮箱实测 SMTP 成功路径；浏览器实测 Admin 页与窄屏。

### v4.7 验证记录（2026-11-11，均实测）
- 复现（未修复代码，`logs/repro_93.py`，scratch 实例 8789 + 临时库，不动 live 库）：dev-a 建 L1 + T1（标题 Y）→ dev-b 全量拉取 cursor=2 → dev-b 偏慢时钟改 T1 为 X-old（updated_at-60000）再推：`conflicts=1`、apply 为空、cursor=2，服务端仍为 Y——`RESULT: BUG-REPRODUCED`；服务端单测 `rejected_record_authoritative_version_in_apply` 同样失败（`apply 应包含被拒记录的服务端权威版本`）；客户端单测证明第 1 轮（apply 为空）本地留 X-old 且 dirty 已清。
- 修复后重跑同一脚本：`conflicts=1`、apply 含 T1（标题 Y）、cursor=2——`RESULT: FIXED`；回归脚本 `logs/regress_93.py` 16/16 通过（401 无/错令牌、429 同设备 5 秒内重复、LWW 双向顺序及反向被拒后 apply 含权威 Y、tombstone 传播、增量只拉新、首次全量、被拒列表权威版本随 apply 下发且 cursor 不推进）。
- 工程：`cargo test --workspace` 24 passed（core 20 + server 4，全绿；之前 22，新增 2 个）；`cargo clippy --all-targets -- -D warnings` 无告警；`cargo fmt --check` 通过；`cargo build --release -p todo-server` 与 `-p todo-app` 通过（server 8.8MB、app 12M）；环境事故：开工时环境又重启过，/etc 回滚致 systemd 单元丢失、服务 inactive，已从仓库正本重装并启动（/healthz ok，数据完好）；桌面 release 构建一度因 apt 依赖回滚链接失败（-lgtk-3/-lwebkit2gtk-4.1 等），已从本地缓存重装（libwebkit2gtk-4.1-dev 等）后通过。
- 线上验证：`systemctl restart todo-server`，`/healthz` ok；curl 真实同步验证（新设备 cursor=0）：cursor=39、apply=4 列表/30 任务、generation=3160ee5a；库状态不变（4 列表/30 任务，回归测试数据）。

## 环境
- Node：v24.22.1 / npm 10.9.4；pnpm 10.34.9（装在 `~/.npm-global/bin`，持久化；此前 npm 全局安装曾被运行时重启回滚）
- Rust：rustc 1.99.1 / cargo 1.99.1（rustup stable，~/.cargo/bin，需自行加入 PATH；另装 clippy + rustfmt 组件）
- 系统：Ubuntu 24.04.5 LTS（root）；Tauri Linux 依赖已装：libwebkit2gtk-4.1-dev 2.52.6、libgtk-3-dev、libayatana-appindicator3-dev、librsvg2-dev、libsecret-1-dev、libssl-dev
- 注意：中途发生一次运行时重启，/tmp 与 apt 安装的系统包被回滚（libzstd1 回到 +hatch1、pnpm 全局安装丢失），~/workspace 与 ~/.cargo 不受影响；已重装恢复。根因：镜像自带 libzstd1（+hatch1）与官方 libzstd-dev 的精确版本要求冲突，解法是同事务内把 libzstd1 钉到官方同版本 1.5.5+dfsg2-2build1.1（上游相同，仅去掉后缀），装后 apt-get check 正常。
- Android（第 2 轮，2026-11-11）：JDK Temurin 17.0.21（`~/workspace/jdk-17`）；Android SDK（`~/workspace/android-sdk`，ANDROID_HOME/ANDROID_SDK_ROOT）：cmdline-tools latest、platform-tools 37.0.3、platforms android-34/android-36、build-tools 34.0.0/36.0.0、NDK r26d 26.3.11579264（NDK_HOME 指向它）；Gradle 9.6.1（预置在 `~/.gradle/wrapper/dists/gradle-9.6.1-bin/4ticwg1pgcbps2hj28r8so764/`）；AGP 9.3.1、Kotlin 2.2.10；Android Rust targets：aarch64/armv7/x86_64/i686-linux-android；cargo-ndk 4.1.2；环境脚本 `~/workspace/todo-app/android-env.sh`（source 后导出全部变量并写 Gradle 代理配置）；Kotlin 编译器 2.2.10（`~/workspace/kotlin-2.2.10`，仅预检用）；已下载的 SDK zip 保留在 `~/workspace/android-sdk/dl/`（约 766M，供重装复用）
- 注意：`src-tauri/gen/android/` 是 `tauri android init` 生成的目录，但本轮在其中有手写改动：`app/build.gradle.kts`（SDK 36）、`gradle.properties`（jvmargs）、`app/src/main/AndroidManifest.xml`（POST_NOTIFICATIONS）、`app/src/main/java/com/example/todo/MainActivity.kt`（权限申请）、`app/src/main/java/com/example/todo/TokenStore.kt`（新建）。若日后删 gen 重 init，这些改动需恢复。
- 服务端（第 3 轮，2026-11-11）：`crates/server/`，axum 0.7.9 / tokio 1 / lettre 0.11 / argon2 0.5 / aes-gcm 0.10 / chrono-tz 0.10 / hmac 0.12 / sha2 0.10 / base64 0.22 / rand 0.8（新增依赖清单，见 crates/server/Cargo.toml）；开发凭据只在 `~/workspace/todo-app/.env`（0600，已进 .gitignore），含 TODO_MASTER_KEY / TODO_ADMIN_PASSWORD / TODO_SYNC_TOKEN / TODO_PORT=8787 / TODO_DB_PATH=<项目>/data/server.db / TODO_INSECURE_HTTP=1 / TODO_POLL_INTERVAL=60——密钥值绝不写入文档与日志；启动：`./target/release/todo-server`（在项目目录下运行，自动读 .env；或 `set -a; source .env; ./target/release/todo-server`）；服务端现由 systemd 管理（`todo-server.service`，已 enable；`systemctl status todo-server`，端口 8787），库 `data/server.db` 为修复轮次后状态（4 列表、30 任务、rev_counter=39、generation=3160ee5a，含回归测试数据 LW1/OG1/TB1/OR1/OFF1/RM1 与恢复回来的 M12 任务；提醒台账 3 行、冲突日志 1 行，见修复轮次验证记录）。
- 桌面 apt 依赖（2026-11-11 13:25 续跑恢复）：运行时重启回滚后，平台 OS-intent 回放先装回 libxcursor-dev 并释放 dpkg 锁（回放进行时不要抢锁、不要 kill）；六个 Tauri 依赖的 .deb 均已在 /var/cache/apt/archives/ 本地缓存（55M），无需 apt-get update，直接 `apt-get install -y --no-download --allow-downgrades libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev libzstd1=1.5.5+dfsg2-2build1.1` 约 2.4 秒装完（libsecret-1-dev、libssl-dev 回滚后仍在）；libzstd1 由 +hatch1 降回官方 1.5.5+dfsg2-2build1.1，`apt-get check` 正常。
- Docker（第 4 轮 M11，2026-11-11）：`docker.io 29.1.3`、`docker-buildx 0.30.1`（apt）、compose v5.7.0（apt 无 compose 插件，从 GitHub releases 下的官方二进制，放 `/usr/libexec/docker/cli-plugins/docker-compose`）；dockerd 须手动起：`GODEBUG=http2client=0 dockerd --storage-driver native --iptables=false --bridge=none --containerd=/run/containerd/containerd.sock --add-runtime no-new-keyring=<wrapper> --default-runtime no-new-keyring`（systemd drop-in 在 `/etc/systemd/system/docker.service.d/m11-override.conf`，/etc 持久性未知，重启后可能丢失；containerd 重启后需先 `systemctl start containerd`）；runc wrapper 与 runc 1.1.14 二进制在 `~/workspace/scratch/`，基础镜像缓存 `~/workspace/scratch/docker-images/`（blobs 保留，tar 已删）；**本机无法运行任何容器**（netlink 被禁，见验证记录 M11），镜像构建与容器运行须换宿主机
- 注意：`/tmp` 是 512M tmpfs，重启清空且极易写满——不要把镜像、大缓存放 `/tmp`（M11 曾因 rust-dl 454M + debian-dl 54M 写满导致父代理提醒验收脚本失败）；大文件一律放 `~/workspace/scratch/`
- 运行时重启（第二次，2026-11-11 约 07:23 UTC，第 4 轮 M12 进行中）：VM 再次重启（uptime 归零），`/tmp` 清空、apt 装的 Tauri 依赖第三次被回滚（`libgtk-3-dev` 等丢失，`cargo build -p todo-app` 链接失败），服务端、Webhook 接收器、调试 SMTP 全停；`/var/cache/apt/archives/` 的 152 个 .deb 缓存未丢，约 5 秒从缓存重装恢复（同上命令）；服务端改走 systemd 管理后不再受 nohup 随重启停的影响（但 unit 文件在 `/etc`，持久性未知，丢了从仓库 `deploy/` 正本重装）。另：`/tmp` 写满时服务端重起失败过一次（stdout 指向 `/tmp` 的日志写不下去，进程起不来）——测试脚本与服务的日志一律不要放 `/tmp`。

## 验证记录（2026-11-11，均实测）
- `cargo test`（workspace）：11 passed（core），其余 crate 0 测试，全绿
- `cargo build -p todo-app`：通过，target/debug/todo-app（238MB debug）
- `cargo clippy --all-targets -- -D warnings`：无告警；`cargo fmt --check -p todo-core -p todo-app`：通过；非测试代码无 unwrap()
- `pnpm build`（tsc + vite build）：通过，dist 产物 253KB JS
- `pnpm dev`：http://localhost:1420/ HTTP 200
- `pnpm tauri dev`（xvfb-run）：vite ready + 编译 + 窗口启动，运行至 100s 超时掐断（exit 124 属预期）
- 桌面二进制直跑（xvfb-run，25s）：存活无崩溃（仅无头 EGL DRI3 警告），建库验证：lists 有收件箱 dirty=1，settings 有 device_id，五张表齐全

### 第 2 轮 M6（2026-11-11，均实测）
- `cargo test -p todo-core`：11 passed，全绿
- `cargo ndk --target aarch64-linux-android check -p todo-app`：通过，无警告（格式化后复核仍通过）
- `cargo clippy --all-targets -- -D warnings`（桌面）：无告警；`cargo ndk --target aarch64-linux-android clippy -p todo-app -- -D warnings`：无告警；`cargo fmt --check -p todo-core -p todo-app`：通过（android_secret.rs 格式已修）
- `pnpm build`：通过，dist JS 253.72 kB；产物 CSS 含 `@media(min-width:48rem)` 断点（<768px 单列 + 底部导航 + 抽屉，≥768px 侧边栏），与 `src/App.tsx` 一致
- `kotlinc 2.2.10` 对 android-36：`TokenStore.kt` 编译通过；`MainActivity.kt` + `TokenStore.kt`（配真实签名的 stub）编译通过
- `pnpm tauri android build --apk -d -t aarch64 --ci`：Rust aarch64 编译通过（`.so` 已 symlink 到 `gen/android/app/src/main/jniLibs/arm64-v8a/`），Gradle 阶段失败——`kotlin-dsl` 插件 6.6.4 下载失败，`--debug` 确认沙箱拦截 Java 进程 TCP（见 M6 阻塞）。APK 未产出；未在真机/模拟器安装验证（未声称已验证）
- `cargo build -p todo-app`（桌面）：失败，`rust-lld: unable to find library -lgdk_pixbuf-2.0`——第 1 轮装的 Tauri Linux 依赖被运行时重启回滚；apt 重装任务以 SIGTERM 终止（约 52 分钟，`apt-get update` 始终未完成；`dpkg -l` 确认 libwebkit2gtk-4.1-dev / libgtk-3-dev 未装，/var/lib/apt/lists/ 仅 3 项），桌面链接验证仍待补（需另行重装 apt 依赖）——2026-11-11 13:25 续跑已恢复，见下两条
- 2026-11-11 13:25 续跑（Android）：`pnpm tauri android build --apk -d -t aarch64 --ci`——pnpm build 通过（dist JS 253.72 kB）、Rust aarch64 编译通过（20.84s）、.so 已 symlink 到 jniLibs/arm64-v8a，Gradle 配置阶段失败（BUILD FAILED in 13s，BUILD_EXIT=1），APK 未产出；`./gradlew help --debug` 复核：代理原文同 M6 阻塞（1 次），DEBUG_EXIT=1；Java Socket 直连探针 TCP-OK；curl 直连 000（SSL wrong version number）、走代理 200
- 2026-11-11 13:25 续跑（桌面）：`apt-get install -y --no-download --allow-downgrades` 六个依赖全部装好（APT_EXIT=0）；`cargo build -p todo-app` 通过（15.44s）；`cargo clippy --all-targets -- -D warnings` 无告警（CLIPPY_EXIT=0）；`cargo test -p todo-core`：11 passed，全绿；`apt-get check` 正常；仓库内无 .apk 文件

### 第 3 轮 M7/M8/M9/M5（2026-11-11，均实测）
- `cargo test`（workspace）：17 passed（core 14 + server 3：crypto 加解密/常量时间比较/HMAC），全绿
- `cargo build -p todo-server` 与 `cargo build -p todo-app`：通过；`cargo clippy --all-targets -- -D warnings`：无告警；`cargo fmt --check -p todo-core -p todo-server -p todo-app`：通过；`pnpm build`：通过（dist JS 254.69 kB）；非测试代码无 unwrap()
- 服务端 release 二进制 8.8MB（≤25MB）；常驻内存 RSS 约 7.3–9.5MB（≤80MB）；100 条变更推送往返约 30ms（≤1s，含本机网络）；dev-c 首次全量拉取 44ms
- curl 同步（/tmp/test_sync.py，24/24 通过，含 429 与 401 探针）：A 首次同步全量拉取、增量推拉、旧值被拒且计 conflict（双向顺序各一遍）、同时间异内容 origin 大者胜并收敛、tombstone 传播、cursor 丢失后全量重推收敛、时钟漂移 +300000ms 回传；Admin 登录 200、devices/conflicts/runs/ledger/notify 均 200 且未登录 401；test/mail 未配 SMTP 时 400 友好报错、test/webhook 未配地址时 400 友好报错
- 提醒（本地 webhook 接收器 127.0.0.1:8899，验签 HMAC-SHA256 3/3 通过）：到期任务只发一次（第 2 个 tick 不重发，ledger status=1 attempts=1）；停机 90 分钟（catch_up 120 内）补发一次（task.overdue）；停机 5 小时不补发；已完成任务不提醒；改期后新 anchor 再发一次（task.due，台账新增一行）；重启服务端后不重发（rem1=2 次、rem4=1 次计数不变）；runs 表每 tick 有记录（scanned/enqueued/sent/failed）
- 桌面端真实同步（`pnpm tauri dev`，xvfb）：启动时自动同步，dirty=2（收件箱 + 桌面任务 desk-1）→ 服务端 cursor=2、conflicts=0；桌面库 desk-1 dirty 清 0、sync_client_state cursor=2 lastResult=ok clockOffsetMs=-32；另一客户端 dev-c 首次同步拉到收件箱 + desk-1（44ms）；直接跑 `target/debug/todo-app` 二进制未触发同步（前端未加载，见 M5 遗留）

### 第 4 轮 M10 + 环境（2026-11-11，均实测）
- 服务停因查清：VM 在 2026-11-11 约 06:04 UTC 重启过（uptime 54 分钟），服务端是 nohup 前台进程、无保活，随重启一起停了（runs 停在 32）；已用新 release 二进制重启（PID 8371，/healthz ok，lagSeconds 3）
- APK 第 4 轮重测：`pnpm tauri android build --apk -d -t aarch64 --ci` EXIT=1，Gradle 配置阶段 kotlin-dsl 6.6.4 找不到，无 APK；直接 `--debug` 在 daemon 分发处失败（见 M6），未再捕获代理原文
- M10（scratch 实例 8789 + 临时库 /tmp/admin-test.db，不动主库）：/admin 200（三 tab 齐全）；登录错密 401、对密 200；无 Cookie 调 /api/devices 401、有 Cookie 200；GET/PUT /api/notify 200；无 webhook 接收器时 /api/test/webhook 502 友好报错；GET /api/token hasToken=true；POST /api/token 生成新令牌（43 字符，值未记录）后旧令牌 ping 401、新令牌 ping 200；重启 scratch 服务后旧 Cookie 调 /api/devices 仍 200（会话落库生效）；logout 后 200、再调 401，sessions 表 0 行；内嵌 JS `node --check` 通过
- `cargo build -p todo-server`（debug + release 8.8MB）通过；`cargo clippy -p todo-server --all-targets -- -D warnings` 无告警；`cargo fmt --check -p todo-server` 通过；`cargo test -p todo-server` 3 passed

### 第 4 轮 M11（2026-11-11，均实测）
- Docker：`docker.io 29.1.3-0ubuntu3~24.04.2`、`docker-buildx 0.30.1`（apt）、compose v5.7.0（apt 无 compose 插件，从 GitHub releases 下的官方二进制，放 `/usr/libexec/docker/cli-plugins/docker-compose`）；`docker compose config -q` 通过，缺 `TODO_MASTER_KEY` 时报错退出（必填校验生效）；解析后服务：`todo-server:latest`、`8787:8787`、`todo-data:/data`、`restart: unless-stopped`、7 个环境变量键齐全（值未记录）
- `systemd-analyze verify deploy/todo-server.service` 通过（仅系统自带 motd-news/apt-daily timer 与 syslog dropin 警告，与本 unit 无关）；未做 live 重启测试（按任务要求，由父代理统一测）
- `docker buildx build --check`：Check complete, no warnings found
- `docker build -t todo-server:test`：失败，停在首个 RUN（runtime 阶段 `apt-get install ca-certificates`），报错 `runc run failed: unable to start container process: error during container init`——容器无法启动，见下；基础镜像解析与上下文发送均已通过，Dockerfile 本身无问题
- 基础镜像获取：dockerd 直接拉镜像失败——Go 的 TLS 经出口代理握手超时（curl/python 正常），`GODEBUG=http2client=0` 后 `registry-1.docker.io` 通但 CloudFront blob 仍超时；Docker Hub manifest 单次曾耗时 195s、blob 约 2.4MB/s；改用 curl/python 调 Registry API 下载后 `docker load` 载入（脚本 `~/workspace/scratch/pull_curl.sh`）：`debian:bookworm-slim` 114MB、`rust:1.99-bookworm`（native 快照计 3.88GB）
- 沙箱限制（均实测）：overlayfs 快照带 lowerdir 挂载 EINVAL（嵌套 overlay + `nouserxattr`）→ 改用 `--storage-driver native`；`--iptables=false --bridge=none`（netfilter 被禁，dockerd 建 NAT 链失败）；runc 1.2+/1.3 的 mount remapping 线程 `setns(CLONE_NEWNS)` EPERM → 试过 runc 1.1.14（仍 EPERM）；session keyring 创建 EPERM → `--no-new-keyring` wrapper；ptrace 对 shim 子进程 EPERM（strace 不可用）；最终根因：**netlink 被禁**——systemd 单元内 `unshare --net; ip link set lo up` 亦报 `RTNETLINK answers: Operation not permitted`，`--network host` 时 dockerd 报 `failed to create default sandbox`；`--privileged` 与 `seccomp=unconfined` 同样失败。结论：本机无法运行任何容器，build/容器测试须换宿主机
- 环境事故：07:2x 运行时重启回滚 apt 包（docker/buildx/compose 插件、/tmp 全部丢失），`~/workspace` 与 `~/.cargo` 完好；重启后 OS-intent 回放自动把 `docker.io 29.1.3` 与 `docker-buildx 0.30.1` 装回（期间不要抢 dpkg 锁），compose 插件需手动重下，containerd 需 `systemctl start containerd` 后再起 dockerd；`/tmp` 是 512M tmpfs，曾被镜像缓存写满（rust-dl 454M + debian-dl 54M）导致父代理提醒验收脚本失败，已清理 `/tmp` 并把缓存移到 `~/workspace/scratch/docker-images/`（镜像 tar 已删，仅留 blobs，可重组）；`todo-server` 在重启后未自动恢复（07:23 检查时 `/healthz` 连接被拒，需父代理重启，本轮未碰服务、未动数据库）

### 第 4 轮 M12（2026-11-11，均实测；验收前已清空测试数据重建干净库，旧库备份在 `data/backup/server.db.bak-20261111-0708`）
- 同步语义（`/tmp/accept_sync.py`，已随 VM 重启丢失；结果 22/22 全通过）：S1 首次全量拉取（dev-b cursor=0 拉到 L1+T1）；S2 增量只拉到 T2；S3 LWW 双向顺序——A 先推旧 X、B 后推新 Y → 双方 Y，B 先推新 Y、A 后推旧 X → 被拒 conflicts=1、conflict_log 胜 dev-b/败 dev-a、双方仍 Y；S4 同时间异内容 origin 大者胜（双向：m<z 被拒、z>m 胜出）；S5 tombstone 传播（B 拉到 `deletedAt` 非空）；S6 时钟漂移 clockOffsetMs=299999（devices 表同值）；S7 cursor=0 全量拉回 7 条、重推 T1 无重复；S8 无令牌 401、错令牌 401、5 秒内重复同步 429。§17 第二行“反向顺序双方均为 X”与 §9.2 新者胜矛盾，实测以新者胜为准，见 M12 清单。
- 提醒（本地 webhook 接收器 127.0.0.1:8899 + 本地调试 SMTP 127.0.0.1:8025 STARTTLS 自签证书；脚本 `/tmp/accept_remind.py` 与 `logs/accept_remind2.py`，结果全通过）：R1 到期发 webhook 一次（台账 webhook status=1 attempts=1，第 2 个 tick 不重发）；R2 改期后新 anchor 再发一次（台账新增一行）；R3 已完成任务 0 事件、0 台账行；R4 过期 90 分钟补发 task.overdue 一次；R5 过期 5 小时 0 事件、0 台账行；R6 两次重启后 R1=2 次、R4=1 次计数不变、mail attempts 仍为 1、tasks=11 数据完好；R7 mail 台账 status=2 attempts=1，错误 `invalid peer certificate: Other(CaUsedAsEndEntity)`，调试 SMTP 收到 6 次连接（lettre 真尝试发信，失败路径）；R8 每日摘要 digest.daily 一次，台账 `__digest__` anchor=20261111 status=1，不重复；F Admin `/api/ledger` 8 行、`/api/conflicts` 3 行、`/api/runs` 21 行均可见。Webhook HMAC-SHA256 验签 sig_ok=true（ping 与提醒事件均验证）；SMTP 成功路径未用真实邮箱实测（lettre 用 webpki-roots，自签证书必被拒）；Webhook 5xx 重试 3 次的定时未实测。
- 服务端清库重建（`logs/restore_push.py`）：停服务、备份、删库、重起后 tasks=0/rev=0；cursor=0 重推 12 条（1 列表 + 11 任务，含 tombstone TD），零冲突；新设备全量拉回 12 条一致；rev_counter=12。PASS。
- 桌面端真实同步（`pnpm tauri dev`，xvfb）：旧 cursor=2 对重建后服务端——`sync round 0: ok cursor=12 apply=10`，但 10 条因所属列表 L1（rev=1）未拉到而全部跳过、cursor 已推进到 12、状态 ok，客户端静默缺失 L1 与 10 条任务且后续同步永不补回（`apply_sync` 的“列表还没拉到，跳过，下轮补齐”在此场景不成立）；手动把客户端 cursor 置 0 后重启——`apply=12`，拉回 L1 + 全部 11 条任务，cursor=12，状态 ok，恢复通过。离线：服务端停机时启动同步失败（last_result=error，`无法连接服务端`），dirty=1 的离线任务保留不丢；服务端恢复后重启客户端，启动同步推送成功（服务端 tasks=12），客户端 dirty 清 0、cursor=13、状态 ok。离线新建经 DB 层模拟（与 UI 调用的 `repo.create_task` 同写路径，dirty=1），UI 点按未实测（无浏览器驱动）。
- 打包产物（`pnpm tauri build`）：通过，`target/release/todo-app` 11.98MB；产物 deb 4.73MiB、rpm 4.73MiB、AppImage 79.6MiB。直接跑 release 二进制（xvfb）：前端正常加载，启动同步 `sync round 0: ok cursor=13 apply=0`，客户端 last_sync_at 更新、状态 ok——第 1/3 轮“直接跑二进制前端未加载”遗留至此闭合（release 产物；debug 二进制直跑未再单独复测）。
- 列表拖拽回归：`src/store.ts` 新增 `reorderList` + `Sidebar.tsx` 接入 HTML5 拖拽；`pnpm build` 通过；`cargo test -p todo-core` 15 passed（原 14，+1 `reorder_list_changes_order`）。桌面端拖拽实测未做（无浏览器驱动），回归限于构建 + 单测。
- 工程：`cargo test --workspace` 18 passed（core 15 + server 3）；`cargo build -p todo-server` 与 `-p todo-app` 通过；`cargo clippy --all-targets -- -D warnings` 无告警；`cargo fmt --check -p todo-core -p todo-server -p todo-app` 通过；`pnpm build` 通过；非测试代码无 `unwrap()`。
- systemd 保活 live 测试（由本轮亲自测）：`cp deploy/todo-server.service /etc/systemd/system/` + `daemon-reload` + `enable`；停 nohup 服务后 `systemctl start` → active、`/healthz` ok、tasks=12 数据完好（含 off-1）；`systemctl restart` → active（MainPID=15831，Restart=on-failure）、`/healthz` ok、tasks=12。服务现由 systemd 管理。
- 环境事故（M12 进行中）：`/tmp` 曾被 M11 镜像缓存写满（512M tmpfs 100%），导致提醒验收脚本内服务端重起失败、服务停约 10 分钟——接收器/SMTP/脚本日志已改走项目 `logs/` 目录，服务手动重起恢复；随后 VM 第二次重启（约 07:23 UTC，uptime 归零），`/tmp` 清空、apt 依赖第三次回滚（从 `/var/cache/apt/archives/` 152 个 .deb 约 5 秒重装恢复），服务与接收器全部重起；Webhook 密钥因旧密钥文件随 `/tmp` 丢失而轮换一次（Admin PUT 新密钥，ping 验签 sig_ok=true）。测试脚本里两个小 bug（`wait_for` 未捕获异常、Python cookiejar 不回送 Secure Cookie）均为测试工具问题，非服务端 bug，已修正。
