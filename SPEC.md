# 待办（Todo）多端应用 + 同步/提醒服务端 · SPEC v4.0

> 客户端：Tauri 2 · React 19 · TypeScript · SQLite（Windows / Android）
> 服务端：Rust · axum · SQLite（同步中枢 + 定时邮件 / Webhook 推送）
> 日期：2026-11-09

---

## 0. 这份文档怎么用

1. 唯一事实来源。开工前通读，**不要**靠扫描项目猜架构。
2. 冲突时以本文件为准。改设计 → 先改本文件再改代码，文末追加变更记录。
3. 每完成里程碑只更新 `PROGRESS.md`（§16），不要重述 SPEC。
4. 按 §15 里程碑分段推进，每段只读相关章节。

---

## 1. 定位与架构

**一句话**：双端待办应用，本地 SQLite 离线优先；服务端是**同步中枢**（记录级 LWW 增量同步）兼**提醒引擎**（定时邮件 + Webhook）。

### 1.1 同步模型（本版核心变更）

**每条记录都带 `updated_at`（最后修改时间，毫秒），同步时逐条比对，新的覆盖旧的。**

| 角色 | 职责 |
|---|---|
| 客户端 | 数据的**产生方**。本地 SQLite 是唯一工作副本，离线可完整使用；变更打 `dirty` 标记，联网时推给服务端 |
| 服务端 | 同步的**权威方**。逐条接收客户端变更，按 `updated_at` 比对合并；向其他设备下发增量 |
| WebDAV | **不再是同步通道**。降级为 P1 可选备份（把服务端数据导出快照到用户自己的 WebDAV） |

**代价与前提（必须明确）**：

- 多端同步**必须**有服务端。没有服务端时，客户端单机功能完整可用，但多端不可同步。
- 提醒、同步、邮件、Webhook 全部依赖服务端在线。
- LWW 意味着**旧修改会被新修改静默覆盖**（两端离线各改同一条时）。服务端记冲突日志，P1 提供查看；不做交互式合并。

### 1.2 数据流

```text
┌──────────────┐   增量推送/拉取（记录级 LWW）   ┌────────────────────────┐
│ Windows 客户端 │ ←──────────────────────────→ │                        │
└──────────────┘                              │   服务端                │
┌──────────────┐                              │  ├─ 同步中枢（权威库）    │
│ Android 客户端 │ ←──────────────────────────→ │  ├─ 提醒调度（读自家库）  │
└──────────────┘                              │  └─ Admin 页            │
                                              └────┬──────────────┬─────┘
                                                   ↓              ↓
                                                 SMTP          Webhook
```

对比 v3 的关键简化：**服务端不再需要轮询 WebDAV 快照**——它自己就是权威库，提醒计算直接查库，延迟从"分钟级"降到"秒级"。

### 1.3 明确不做（Scope Guard）

- WebDAV / 任何无服务端的对等同步（P1 仅保留"导出备份"）
- 交互式冲突合并（三路合并、字段级 merge）——只用 LWW + 冲突日志
- 实时推送（WebSocket 长连接）；同步为手动 + 定时触发
- 自建注册体系：MVP 单工作区，接入靠同步令牌；暂不设 `workspace_id` 列，多工作区以后再加
- 推送通道（APNs / FCM）
- 遥测、崩溃上报
- 客户端之间点对点直连

---

## 2. 技术栈与仓库结构

### 2.1 选型

| 层 | 选型 | 说明 |
|---|---|---|
| 客户端壳 | Tauri 2.x | desktop + android |
| 前端 | React 19 + TS 5 + Tailwind 4 + Zustand 5 | 不引入 UI 库 |
| 客户端 DB | rusqlite（bundled） | |
| 服务端 | Rust + axum 0.7 + tokio | 单二进制 |
| 服务端 DB | SQLite（rusqlite） | 单写者（WAL） |
| HTTP | reqwest（rustls） | 仅 Webhook 等外发 |
| 邮件 | lettre（rustls） | |
| 时区 | chrono + chrono-tz | |
| Admin UI | 内嵌单页 HTML + 原生 JS（include_str! 内嵌） | 不引第二套前端构建链 |

服务端用 Rust 的理由不变：`core` crate 的 `models` / `db` / LWW 合并逻辑客户端服务端共用，避免语义漂移。

### 2.2 仓库结构

```text
todo-app/
├── crates/
│   ├── core/                     # 无 Tauri 依赖，双端共用
│   │   └── src/
│   │       ├── models.rs         # Task / TodoList / 同步载荷结构
│   │       ├── db/{mod.rs,schema.sql,repo.rs}
│   │       ├── lww.rs            # 逐条 LWW 比对（纯函数，双端共用，必须单测覆盖）
│   │       └── error.rs
│   └── server/
│       └── src/
│           ├── main.rs
│           ├── db/{mod.rs,schema.sql,repo.rs}
│           ├── api/{mod.rs,auth.rs,sync.rs,config.rs,logs.rs}
│           ├── jobs/{scheduler.rs,dispatcher.rs}
│           ├── crypto.rs         # AES-256-GCM
│           ├── state.rs
│           └── admin.html        # Admin 单页，include_str! 内嵌
├── src/                          # 客户端前端
│   └── ...（沿用）+ settings/ServerPane.tsx
├── src-tauri/
│   └── src/{main.rs,lib.rs,commands.rs,sync_client.rs,secret.rs,android_secret.rs,local_notify.rs}
├── Dockerfile / docker-compose.yml
├── SPEC.md / PROGRESS.md / README.md
```

**硬性约束**

- `crates/core` 不依赖 `tauri`；LWW 比对必须是纯函数、零 IO。
- 客户端 SQL 只在 `core/db/repo.rs`，服务端 SQL 只在 `server/db/repo.rs`。
- 同步协议结构体只定义一次（`core/models.rs`），两端 serde 序列化必须一致（单测对拍）。
- 出站请求只允许：服务端 → SMTP / Webhook /（P1）WebDAV。

---

## 3. 客户端数据模型

```sql
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS schema_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);

CREATE TABLE IF NOT EXISTS lists (
  id TEXT PRIMARY KEY, name TEXT NOT NULL, color TEXT NOT NULL DEFAULT '#378ADD',
  sort_order INTEGER NOT NULL DEFAULT 0,
  deleted_at INTEGER,
  dirty INTEGER NOT NULL DEFAULT 1,          -- 1=未同步
  updated_at INTEGER NOT NULL,               -- 最后修改时间，LWW 依据
  origin TEXT NOT NULL DEFAULT '',           -- 最后写入的 deviceId
  created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS tasks (
  id TEXT PRIMARY KEY,
  list_id TEXT NOT NULL REFERENCES lists(id) ON DELETE CASCADE,
  title TEXT NOT NULL, notes TEXT NOT NULL DEFAULT '',
  status INTEGER NOT NULL DEFAULT 0, priority INTEGER NOT NULL DEFAULT 0,
  due_at INTEGER, completed_at INTEGER,
  sort_order REAL NOT NULL DEFAULT 0,
  deleted_at INTEGER,                        -- 软删除 = tombstone，必须参与同步
  dirty INTEGER NOT NULL DEFAULT 1,
  updated_at INTEGER NOT NULL,
  origin TEXT NOT NULL DEFAULT '',
  created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_tasks_list_status ON tasks(list_id, status, sort_order);
CREATE INDEX IF NOT EXISTS idx_tasks_due ON tasks(due_at) WHERE due_at IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_tasks_dirty ON tasks(dirty) WHERE dirty = 1;

CREATE TABLE IF NOT EXISTS sync_client_state (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  cursor INTEGER NOT NULL DEFAULT 0,         -- 服务端下发，单调递增
  generation TEXT,                           -- 上次见到的服务端 generation（v4.4）
  last_sync_at INTEGER,
  last_result TEXT,                          -- ok | conflict | error
  last_error TEXT,
  clock_offset_ms INTEGER                    -- 上次同步测得的与服务端时差
);

CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL, updated_at INTEGER NOT NULL);
```

**约定**

- **任何写操作（本地增删改）必须同时：`updated_at = now`、`dirty = 1`、`origin = 本机 deviceId`。** 封装成 `repo.rs` 的统一写入口，禁止绕过。
- `id`：Rust 侧 uuid v4。
- 删除一律软删除（`deleted_at`）。tombstone 在**收到服务端确认后**保留 90 天，本地过期清理；硬删除（Shift+Delete）仅对 `dirty=0` 的已同步记录允许。
- 服务器时间漂移检测见 §9.4。
- 首次启动：插入"收件箱"（dirty=1）。

---

## 4. 服务端数据模型

```sql
PRAGMA journal_mode = WAL;

-- 权威业务库（与客户端同构，外加 server_rev）
CREATE TABLE IF NOT EXISTS lists (
  id TEXT PRIMARY KEY, name TEXT NOT NULL, color TEXT NOT NULL,
  sort_order INTEGER NOT NULL DEFAULT 0,
  deleted_at INTEGER,
  updated_at INTEGER NOT NULL, origin TEXT NOT NULL DEFAULT '',
  server_rev INTEGER NOT NULL,               -- 全局单调递增，拉取游标依据
  created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS tasks (
  id TEXT PRIMARY KEY,
  list_id TEXT NOT NULL REFERENCES lists(id) ON DELETE CASCADE,
  title TEXT NOT NULL, notes TEXT NOT NULL DEFAULT '',
  status INTEGER NOT NULL DEFAULT 0, priority INTEGER NOT NULL DEFAULT 0,
  due_at INTEGER, completed_at INTEGER,
  sort_order REAL NOT NULL DEFAULT 0,
  deleted_at INTEGER,
  updated_at INTEGER NOT NULL, origin TEXT NOT NULL DEFAULT '',
  server_rev INTEGER NOT NULL,
  created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_tasks_rev ON tasks(server_rev);
CREATE INDEX IF NOT EXISTS idx_tasks_due ON tasks(due_at) WHERE due_at IS NOT NULL AND deleted_at IS NULL;

CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);   -- rev_counter、generation（v4.4：库初始化时生成的 uuid v4）

CREATE TABLE IF NOT EXISTS devices (
  device_id TEXT PRIMARY KEY,
  name TEXT NOT NULL, platform TEXT NOT NULL,
  last_cursor INTEGER NOT NULL DEFAULT 0,
  last_sync_at INTEGER,
  clock_offset_ms INTEGER,
  created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS conflict_log (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  entity TEXT NOT NULL,                      -- task | list
  entity_id TEXT NOT NULL,
  winner_origin TEXT NOT NULL,
  loser_origin TEXT NOT NULL,
  winner_updated_at INTEGER NOT NULL,
  loser_updated_at INTEGER NOT NULL,
  resolved_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS reminder_ledger (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  task_id TEXT NOT NULL,
  kind TEXT NOT NULL,                        -- due | digest
  anchor INTEGER NOT NULL,                   -- due_at 或 yyyymmdd
  offset_min INTEGER NOT NULL DEFAULT 0,
  channel TEXT NOT NULL,                     -- mail | webhook
  status INTEGER NOT NULL DEFAULT 0,         -- 0 pending 1 sent 2 failed
  attempts INTEGER NOT NULL DEFAULT 0,
  sent_at INTEGER, last_error TEXT, created_at INTEGER NOT NULL,
  UNIQUE(task_id, kind, anchor, offset_min, channel)
);

CREATE TABLE IF NOT EXISTS runs (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  started_at INTEGER NOT NULL, finished_at INTEGER,
  scanned INTEGER NOT NULL DEFAULT 0, enqueued INTEGER NOT NULL DEFAULT 0,
  sent INTEGER NOT NULL DEFAULT 0, failed INTEGER NOT NULL DEFAULT 0, note TEXT
);

CREATE TABLE IF NOT EXISTS admin (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  username TEXT NOT NULL, password_hash TEXT NOT NULL, created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS secrets (key TEXT PRIMARY KEY, value_enc BLOB NOT NULL);  -- 同步令牌等
```

**说明**

- `server_rev`：每次对 lists/tasks 的写入，在事务内 `UPDATE meta SET value = value + 1 WHERE key='rev_counter'`，把新值写进记录。拉取按 `server_rev > cursor` 增量下发。
- `generation`（v4.4）：库初始化时生成的 uuid v4，存 `meta key='generation'`；每次 `/api/v1/sync` 响应里带上它，客户端比对，不一致即按首次同步处理（见 §9.5）。清库重建（删库文件）生成新 generation；只删业务表不删 `meta` 的"软清空"不在本轮范围。
- **服务端不保存跨设备"哪台设备知道哪些记录"以外的冗余**；每台设备的进度就是 `devices.last_cursor`（观测用），协议实际以客户端上报的 cursor 为准。
- 提醒台账不再存 workspace 维度（MVP 单工作区）。

---

## 5. 功能需求

### 5.1 客户端（P0）

- 列表与任务 CRUD、搜索、拖拽排序、已完成折叠
- **同步**：手动"立即同步" + 启动时 + 变更后 5 分钟防抖 + 前台每 15 分钟；离线时静默失败、dirty 累积，恢复后自动补传
- 设置页：`服务端`（URL + 同步令牌 + 测试连接 + 自动同步间隔）、`本地通知` 两个分区
- 本地通知：应用前台时，到期按本机偏好提醒
- 响应式：≥768px 侧边栏；<768px 抽屉 + 底部导航

### 5.2 服务端（P0）

| 能力 | 要求 |
|---|---|
| 同步 API | 记录级 LWW 增量（§9），单次往返完成"推 + 拉" |
| 提醒计算 | 直接查权威库：`status=0 AND due_at IS NOT NULL AND deleted_at IS NULL` |
| 邮件 / Webhook | 到期前按偏移发送；每日摘要可选；HMAC 签名 |
| 幂等 | `reminder_ledger` 唯一键，重启不重发 |
| 补偿 | 停机错过提醒在 `catch_up_min`（默认 120 分钟）内补发一次 |
| 改期 | `due_at` 变化 → 新 anchor → 允许再提醒 |
| Admin 页 | 连接 / 通知 / 运行三 tab（沿用 v3） |

### 5.3 P1

- WebDAV 备份导出（服务端定期把权威库导出 JSON 快照上传用户 WebDAV）
- 冲突查看页（`conflict_log`）
- 多工作区
- 邮件内"标记完成"深链（服务端写权威库，技术上已可行）
- 客户端同步历史查看

### 5.4 P2（不做）

- 字段级三路合并、实时协同、注册体系、推送通道

---

## 6. 客户端交互

沿用响应式断点与快捷键。状态区显示同步指示：`idle / syncing / ok(时间) / conflict(N) / error`，hover 显示详情；离线时显示"本地修改 N 条待同步"。

---

## 7. 客户端 IPC

| 命令 | 说明 |
|---|---|
| `list_lists` / `create_list` / `rename_list` / `delete_list` / `reorder_list` | 列表 |
| `list_tasks` / `create_task` / `update_task` / `toggle_task` / `delete_task` / `reorder_task` / `search_tasks` / `counts` | 任务 |
| `get_settings` / `save_settings`（scope: `server` \| `local_notify`） | 设置；令牌只写不读（返回 `hasToken`） |
| `test_server` | 测连通 + 鉴权（`GET /api/v1/ping`） |
| `sync_now` | 立即同步；同步中重复调用返回 `BUSY` |
| `sync_status` | `{ state, lastSyncAt, pendingCount, lastError, clockOffsetMs }` |
| `due_notifications` | 本地通知：查询到期任务（供前台提醒） |
| `reset_sync` | 重置同步：cursor 与 generation 清零、本地全部记录重新标记 dirty，下次同步全量重拉并全量重推（v4.4 兜底入口） |

删除 `test_dav` / `test_mail` / `test_webhook` / `outbox_flush`（均上移服务端）。

---

## 8. 服务端 HTTP API

| 方法 | 路径 | 鉴权 | 说明 |
|---|---|---|---|
| `GET` | `/healthz` | 公开 | `{ ok, lastRunAt, lagSeconds }` |
| `GET` | `/admin` | — | Admin 单页（三 tab） |
| `GET` | `/api/v1/ping` | 同步令牌 | 客户端"测试连接" |
| `POST` | `/api/v1/sync` | 同步令牌 | 核心，见 §9 |
| `POST` | `/api/login` / `/api/logout` | — | Admin |
| `GET/POST` | `/api/token` | Admin | 同步令牌状态（GET）/ 生成重置（POST） |
| `GET/PUT` | `/api/notify` | Admin | SMTP / Webhook / 偏移 / 摘要 |
| `POST` | `/api/test/mail` / `/api/test/webhook` | Admin | 测试发送 |
| `GET` | `/api/devices` | Admin | 设备列表 + 游标 + 时差 |
| `GET` | `/api/conflicts?limit=50` | Admin | 冲突日志（P1 前先落库） |
| `GET` | `/api/runs` / `/api/ledger` | Admin | 运行与台账 |

限频：`/api/v1/sync` 每设备最小间隔 5 秒，违反返回 `429 TOO_FREQUENT`。
鉴权：`Authorization: Bearer <SYNC_TOKEN>`；令牌在 Admin 页生成/重置，客户端存入凭据库。

---

## 9. 同步协议（记录级 LWW）

### 9.1 请求 / 响应

```jsonc
// POST /api/v1/sync
{
  "deviceId": "win-8f3a2c", "deviceName": "桌面机", "platform": "windows",
  "lastCursor": 1024,
  "clientTime": 1762750000000,
  "changes": {
    "lists": [ { "id": "...", "name": "工作", "updatedAt": 1762749999000,
                 "deletedAt": null, "origin": "win-8f3a2c", "color": "#378ADD", "sortOrder": 0 } ],
    "tasks": [ { "id": "...", "listId": "...", "title": "写 SPEC", "notes": "",
                 "status": 0, "priority": 2, "dueAt": 1762838400000, "completedAt": null,
                 "sortOrder": 1.5, "deletedAt": null,
                 "updatedAt": 1762749999000, "origin": "win-8f3a2c" } ]
  }
}

// 200
{
  "cursor": 1088,
  "generation": "9f2c4a1e-…",  // 服务端库代际标识（v4.4，§9.5）；清库重建后变化
  "serverTime": 1762750000123,
  "clockOffsetMs": -123,          // clientTime - serverTime，供客户端告警
  "conflicts": 2,                 // 本轮被服务端拒绝的条数
  "apply": {                      // server_rev > lastCursor 的记录（含其他设备的变更）
    "lists": [ ... ], "tasks": [ ... ]
  }
}
```

`apply` 单次最多 5000 条；超出时客户端拿新 cursor 循环拉取直到 `apply` 为空。

`generation` 是服务端库的代际标识（v4.4）：库初始化时在 `meta` 生成一个 uuid v4，存 `meta key='generation'`；每次 `/api/v1/sync` 响应里带上它。客户端把上次见到的 generation 存进 `sync_client_state`，发现不一致（或本地为空而服务端有值且本地 `cursor>0`）时，立即按首次同步处理：cursor 归零、全量拉取、本地全部记录重新标记 dirty 后重推，LWW 自然收敛。generation 变化不是错误——它就是一次正常的全量重同步触发条件，状态区不要报红，记一条日志即可。清库重建（删库文件）会生成新 generation；只删业务表不删 `meta` 的"软清空"不在本轮范围。

### 9.2 服务端合并规则（逐条、确定性）

对客户端推来的每条记录（lists 与 tasks 同规则）：

```text
服务器不存在该 id            → 插入（bump server_rev）
incoming.updated_at >  服务器 → 覆盖（bump server_rev）
incoming.updated_at <  服务器 → 保留服务器版本，conflict + 1，写 conflict_log
incoming.updated_at == 服务器 → 内容相同：忽略
                               内容不同：origin 字典序大者胜，败方记 conflict_log
```

- 比对字段：`updated_at` + `origin`，**不比对内容 hash**：同时间异内容时只用 `origin` 裁决，规则保持可预测。
- tombstone（`deleted_at` 非空）与普通记录走同一套 LWW：`deleted_at` 也可被更新的修改"复活"。
- 每个写入都在一个事务内：bump 计数器 → 写记录 → 提交。

### 9.3 客户端应用规则

```text
同步流程（单事务）：
1. 收集 dirty 记录（lists + tasks），打包 changes
2. POST /api/v1/sync
3. 服务端"胜出"的已推送记录 → 本地清除 dirty（内容可能与本地不同，响应不逐条回传权威态——
   简化：凡推送过的记录，无论胜负，全部以服务端权威态覆盖本地并清 dirty，
   但仅当本地 updated_at ≤ 推送时的值；若同步往返期间本地又有新改动（dirty 又为 1），保留本地，下轮再推）
4. apply 中的记录 → 本地比对：本地 dirty 且 updated_at 更新 → 保留本地（下轮推送会胜出）；
   否则覆盖本地
5. 写 cursor、clock_offset_ms、sync_client_state
```

- 被拒记录（v4.7）：服务端把被拒记录的服务端权威版本放进 apply（去重，不因此推进 cursor）；客户端在步骤 4 按 `updated_at` 比对覆盖本地。
- 拉取应用也走统一写入口，保证 `dirty` / `origin` / `updated_at` 语义不被破坏。
- **禁止**客户端在同步事务外直接改这三样东西。

### 9.4 时钟

- LWW 依据客户端时钟，**服务端不校正 `updated_at`**（校正会破坏"最后修改"的语义）。
- 防呆：请求带 `clientTime`，服务端回 `clockOffsetMs`；|offset| > 120s 时：
  - 客户端：状态区黄色提示"设备时间不准，可能导致旧数据覆盖新数据"
  - 服务端：`devices.clock_offset_ms` 记录，Admin 页标黄
- 不做 NTP 校时（超出范围）。

### 9.5 边界

| 场景 | 处理 |
|---|---|
| 首次同步 | `lastCursor=0` → 全量拉取 |
| 两台设备离线各改同一条 | 后同步的那台推上去胜出；先同步的设备在下一轮拉取中被覆盖 |
| 删除传播 | tombstone 作为普通记录同步；其他设备拉取后本地也软删 |
| 服务端重启 | `rev_counter` 持久在 meta，游标不错乱 |
| 服务端清库重建 | generation 变化 → 客户端 cursor 归零全量重同步，本地全部记录重推（§9.1） |
| 客户端 cursor 丢失（清数据） | 视为首次同步，全量拉取 + 本地 dirty 重推，LWW 自然收敛 |
| 单设备记录量 | 5000 条/轮上限，客户端循环拉取 |
| 断网 | 同步失败静默，dirty 累积；不阻塞本地任何操作 |

---

## 10. 提醒调度（服务端，读自家库）

每 60s tick：

```text
1. SELECT id, title, due_at FROM tasks
   WHERE status=0 AND deleted_at IS NULL AND due_at IS NOT NULL
2. 候选点 = due_at - offset（offset ∈ remind_offsets）
3. 命中窗口：remind_at ∈ [now - tick, now + tick]；
   错过的：now - remind_at ≤ catch_up_min 才补发，否则丢弃并记日志
4. INSERT OR IGNORE INTO reminder_ledger(...)；
   受影响行数 = 1 才真正发送（唯一键即幂等锁）
5. 发送 mail / webhook，更新台账；写 runs 汇总
```

- 去重键：`UNIQUE(task_id, kind, anchor, offset_min, channel)`；摘要 `anchor=yyyymmdd`，`task_id='__digest__'`。
- 时区：摘要与"今天"判定用 `notify_config` 里的时区（`chrono_tz`）。
- 邮件模板：`[待办] <标题>`（摘要 `[待办] 今日 N 项待办`），纯文本 + HTML，不含笔记全文。
- Webhook：`X-Todo-Event: task.due | task.overdue | digest.daily | ping`，`X-Todo-Signature: sha256=<hmac>`；2xx 成功；4xx（除 408/429）不重试；5xx/超时重试 3 次（30s / 2min / 10min）。
- **改期即时性**：服务端是权威库，任务改期/完成后下一轮 tick（≤60s）即生效，无需等客户端同步。

---

## 11. 安全

| 项 | 规则 |
|---|---|
| 主密钥 | `TODO_MASTER_KEY`（32 字节 base64）环境变量，缺失拒启动 |
| 加密存储 | 同步令牌、SMTP 密码、Webhook 密钥 AES-256-GCM 存 `secrets` / 配置表，nonce 随机 |
| 同步鉴权 | Bearer 令牌；常量时间比较；错误 401 不区分"令牌不存在/不匹配" |
| Admin | 单账号 argon2id；session HttpOnly + SameSite=Lax + Secure，24h |
| 传输 | 生产必须 TLS 反代；`TODO_INSECURE_HTTP=1` 才允许裸 HTTP，启动打警告 |
| 客户端凭据 | `SecretStore`：Windows → keyring；Android → Keystore + AES-GCM |
| 日志 | 脱敏：URL 留 host、邮箱留首字符 + 域名、令牌与密码永不输出 |
| 数据 | 服务端权威库含任务内容 → 备份必须加密或限权；WebDAV 备份导出（P1）必须先询问用户确认明文风险 |

---

## 12. 错误处理

| 场景 | 处理 |
|---|---|
| 同步 401 | 客户端停止自动同步，状态区红色"令牌无效"，提示重新配置 |
| 同步 429 | 静默退避到下个周期 |
| 同步超时（30s） | 整个事务回滚，本地 dirty 不清，下轮重试 |
| apply 中出现未知 `list_id`（服务端列表还没拉到） | 客户端按 `server_rev` 升序应用，列表先于任务；仍缺失则跳过该任务、cursor 不推进过它、状态记 error 并写明跳过条数，下轮重新拉取（v4.4；服务端清库导致列表永不到时由 generation 机制触发全量重同步，§9.5） |
| 服务端库损坏 | `integrity_check` 失败 → 改名备份 + 空库 + Admin 红色告警；客户端 cursor 归零全量重推 |
| 邮件认证失败 | 不重试，台账 failed，错误指明"用授权码而非登录密码" |
| SMTP 端口超时 | 提示"465/25 可能被封，建议 587 + STARTTLS" |
| WebDAV（P1 备份）失败 | 仅记日志，不影响主流程 |
| 设备时钟漂移 > 2 分钟 | 双端提示（§9.4） |

---

## 13. 非功能指标

| 指标 | 目标 |
|---|---|
| 客户端冷启动 | Windows ≤800ms / Android ≤1.2s |
| 服务端内存 | ≤80MB（1 万任务 + 5 设备） |
| 提醒延迟 | ≤ 60s（tick 间隔） |
| 同步往返 | 100 条变更 ≤ 1s（不含网络） |
| 同步往返 | 1 万条全量拉取 ≤ 10s |
| 离线 | 客户端零服务端依赖可完整增删改查 |
| 安装包 | Windows ≤12MB / Android ≤15MB；服务端二进制 ≤25MB |

---

## 14. 构建与部署

### 客户端

```bash
pnpm install
pnpm tauri dev
pnpm tauri build                      # Windows NSIS
pnpm tauri android build --apk        # 需 JDK 17 + NDK r26 + cargo-ndk
```

### 服务端

```bash
cargo build --release -p todo-server
TODO_MASTER_KEY=<base64-32B> TODO_ADMIN_PASSWORD=<初始密码> TODO_INSECURE_HTTP=1 ./todo-server
```

| 变量 | 必填 | 默认 |
|---|---|---|
| `TODO_MASTER_KEY` | ✅ | — |
| `TODO_ADMIN_PASSWORD` | 首次 | — |
| `TODO_SYNC_TOKEN` | 首次（或 Admin 页生成） | — |
| `TODO_DB_PATH` | | `/data/app.db` |
| `TODO_PORT` | | `8787` |
| `TODO_POLL_INTERVAL`（提醒 tick） | | `60` 秒 |
| `TODO_INSECURE_HTTP` | 裸 HTTP 时必须 | 未设为 `1` 时拒绝启动；生产请走 TLS 反代 |

Docker compose 与反代示例沿用：`remind.example.com { reverse_proxy 127.0.0.1:8787 }`。
备份 = 复制 `/data/app.db`（含 WAL）+ 保存主密钥。

---

## 15. 里程碑与预算

| # | 里程碑 | 交付物 | 积分预算 |
|---|---|---|---|
| M0 | 工程重构 | Cargo workspace，`core` 去 Tauri 依赖 | 150–350 |
| M1 | core 数据层 | 双端 schema、repo 统一写入口（dirty/updated_at/origin）、单测 | 450–900 |
| M2 | 客户端主界面 | 响应式布局 + 全链路 CRUD | 450–900 |
| M3 | 客户端交互 | 快捷键、搜索、拖拽、空状态 | 250–550 |
| M4 | 客户端设置 + 本地通知 | ServerPane、SecretStore 双端、系统通知 | 250–550 |
| M5 | 客户端同步引擎 | dirty 收集、cursor、增量推拉、冲突应用、离线补传 | 600–1,200 |
| M6 | Android 适配 | 构建链、平台差异、通知权限、APK 跑通 | 800–1,600 |
| M7 | 服务端骨架 | axum、库、加密、Admin 登录、healthz | 500–1,000 |
| M8 | 服务端同步中枢 | /sync 端点、LWW 合并、rev 游标、conflict_log、限频 | 1,200–2,400 |
| M9 | 服务端提醒 | 调度查库、台账幂等、SMTP、Webhook 签名、重试 | 500–1,000 |
| M10 | Admin 页 | 三 tab、测试发送、设备/冲突/台账查看 | 400–800 |
| M11 | 部署 | Dockerfile、compose、备份与反代文档 | 250–500 |
| M12 | 端到端验收 | 双端 + 服务端联调，按 §17 过 | 300–600 |

**合计约 6,100–12,350 积分，典型 7,500–9,500。**

> **LWW 纯函数（`core/lww.rs`）必须在 M1 就写好并测透**——它是两端共用的唯一裁决逻辑，返工成本最高。

---

## 16. 执行约定（给 AI）

1. 一次改完一次编译；Rust 报错一次给全量（`--message-format short`）。
2. 不复述过程；给路径 + 一句结论。
3. 不重读整个项目；读 `PROGRESS.md` + 相关单文件。
4. 不自造接口；协议以 §9 / §8 为准。
5. 不加依赖（§2）。
6. 不打印凭据；调试用 `[REDACTED]`。
7. 改 `core` 后同时验证 `cargo build -p todo-server` 与 `cargo build -p todo-app`。
8. **同步协议字段增删必须先改 `core/models.rs` + 本文件，再动两端代码。**
9. **代码写给人看。** 命名自解释、函数短小、单一职责；注释用中文，只写"为什么这么做"，不复述代码"做了什么"；禁止为省事用晦涩写法或一行塞多个逻辑。注释是必要的就够，不是越多越好。
10. 每完成里程碑，3 行更新 `PROGRESS.md`。

---

## 17. 验收清单

**客户端**

- [ ] 断网状态下 CRUD 完整可用，dirty 正确累积
- [ ] 恢复网络后自动补传，全部数据到达服务端
- [ ] 首次同步（cursor=0）全量拉取成功
- [ ] 增量同步只传 dirty 记录（服务端 runs/日志可证）
- [ ] 删除任务 → 另一端同步后同样消失（tombstone 传播）
- [ ] 窄屏布局正确

**同步语义（重点）**

- [ ] A 端 12:00 改标题为 X，B 端 12:05 改同一条为 Y → 双方同步后均为 Y
- [ ] 反向顺序（B 先同步）→ 双方均为 Y，A 端推的旧值被服务端拒绝且计 conflict
- [ ] `updated_at` 相同、内容不同 → origin 字典序大者胜，双方收敛一致
- [ ] conflict_log 有对应记录，Admin 可见
- [ ] 客户端时钟快 5 分钟 → 状态区黄色告警，Admin 设备页标黄
- [ ] 清空客户端数据重新同步 → 全量拉回，dirty 重推后收敛，无重复无丢失
- [ ] 服务端清库（模拟损坏重建）→ 客户端 cursor 归零后数据全部恢复

**提醒**

- [ ] 到期任务在偏移点只发一次；重启服务不重发
- [ ] 任务在客户端改期 → 服务端下一轮 tick（≤60s）按新时间提醒
- [ ] 任务完成后不再提醒
- [ ] 停机 90 分钟重启 → 120 分钟内补发一次；停机 5 小时 → 不补发
- [ ] Webhook 签名可验证；SMTP 失败不无限重试

**工程**

- [ ] `cargo build --release -p todo-server` 与 `cargo build -p todo-app` 通过
- [ ] `core/lww.rs` 单测覆盖：新胜旧、旧不胜新、同时间同内容、同时间异内容、tombstone 复活
- [ ] `cargo clippy -- -D warnings` 无告警；`cargo test` 全绿
- [ ] 无 `unwrap()`（测试除外）、日志无明文凭据
- [ ] SPEC 与实现一致

---

## 18. 分期执行（用户优先级：Android > 服务端 > Windows > 同步打磨）

全量约 6,100–12,350 积分，按用户优先级重排为 4 轮，每轮独立开会话、独立收工。

**一个绕不开的依赖**：服务端提醒（邮件/Webhook）的数据来源就是同步中枢——服务端拿不到任务数据就算不出到期。所以"服务端"与"同步"在架构上是一体，轮 3 一次交付两者。你要求的"同步最后做"在执行上的含义是：轮 3 交付**可用的同步 + 提醒**，轮 4 只做打磨（Admin 页、冲突查看、端到端验收）。

| 轮次 | 内容（对应里程碑） | 预算 | 完成后你能做什么 |
|---|---|---|---|
| 1 | 骨架 + 数据层 + 共享 UI（手机优先布局）+ 本地通知（M0–M4） | 1,550–3,250 | 浏览器/桌面 dev 里能记待办，界面按手机布局写 |
| 2 | Android 构建链 + APK + 移动端打磨（M6） | 800–1,600 | 手机上装一个能用的单机待办 App |
| 3 | 服务端（同步中枢 + 邮件/Webhook 提醒）+ 客户端同步接入（M7 M8 M9 M5） | 2,800–5,600 | 手机与电脑数据一致；关掉 App 也有邮件提醒 |
| 4 | Windows 打包 + Admin 页 + 端到端验收 + 部署（M10 M11 M12） | 950–1,900 | 全部交付 |

- **只做轮 1–2**（约 2,350–4,850）：手机上的单机待办 + 本地通知，无服务端。
- **做完轮 3**：同步 + 提醒闭环（约 5,150–10,450）。
- 轮 4 是收尾，隔多久做都不影响已交付功能。

### Android 优先的省积分要点

- 界面迭代用 `pnpm dev`（浏览器）或 `pnpm tauri dev`（桌面窗口），**不要**每改一行就跑 `tauri android dev`——Android 单次构建 5–20 分钟，来回试错会把对话拖得很长。
- Android 构建链的坑（JDK 17 / NDK / cargo-ndk / rusqlite 交叉编译）集中在轮 2 首日解决：先跑通空 APK，再动业务代码。
- 轮 1 的 UI 直接按移动端断点写（<768px 单列 + 底部导航），桌面只是多一个侧边栏媒体查询，返工最少。

### 每轮的开场方式（省积分的关键）

新开会话，只贴这几行，**不要贴整份 SPEC**：

```text
读 SPEC.md 的 §<相关章节> 和 PROGRESS.md，继续做 M<n>。
只改相关文件，不要扫描整个项目。
```

### 每轮收尾必做

3 行更新 `PROGRESS.md`（完成 / 遗留 / 下一步）。这是下一轮唯一交接物，写清楚能省几百积分的重新摸索。

### 止损点

| 位置 | 典型失控 | 对策 |
|---|---|---|
| Rust 编译错误 | 逐条改、逐次编译 | 要求一次给全量报错（`--message-format short`），一次改完 |
| Android 构建链 | 环境问题吃光预算 | 单独一轮；先跑通 `tauri android dev` 再动业务代码 |
| 同步联调 | 客户端服务端来回改 | 先用 curl 把服务端接口测通，客户端最后接 |
| 界面样式 | 无限打磨 | 明确"先能跑通，样式最后统一调" |
| 重复解释需求 | 每轮重新复述架构 | 全靠 SPEC + PROGRESS.md，不口头复述 |

---

## 变更记录

| 日期 | 版本 | 变更 |
|---|---|---|
| 2026-11-09 | v1.0 | 初版（单机 Windows） |
| 2026-11-09 | v2.0 | +Android、WebDAV 同步、客户端邮件/Webhook |
| 2026-11-09 | v3.0 | +提醒服务端（只读 WebDAV 快照），邮件/Webhook 上移 |
| 2026-11-09 | v4.0 | 同步模型改为**服务端中枢 + 记录级 LWW**（每条带 updated_at 逐条比对）；WebDAV 降级为 P1 备份导出；新增 dirty/cursor/server_rev/conflict_log；提醒改为直查权威库；预算 6,300–12,350 |
| 2026-11-09 | v4.1 | 新增 §18 分期执行：7 轮会话切分、单轮预算、止损点（范围不变，仅调整投入节奏） |
| 2026-11-10 | v4.2 | §16 增补代码风格约定：可读性优先，中文注释只写"为什么"，不复述代码 |
| 2026-11-10 | v4.3 | §18 按用户优先级重排：Android > 服务端 > Windows > 同步打磨，7 轮并为 4 轮；注明服务端与同步在架构上一体、须同轮交付 |
| 2026-11-11 | v4.4 | 新增服务端 generation 代际标识与客户端 cursor 重置，修清库后旧 cursor 静默丢数据；generation 变化或手动重置时本地全部记录重推（只重推 dirty 会让新库永久缺失干净记录）；apply 孤儿任务不再推进 cursor，跳过即记 error；§9.1 响应加 generation，§9.5 加清库重建行，§4 meta 加 generation，§3 sync_client_state 加 generation 列，§7 加 reset_sync，§12 孤儿任务行同步改写 |
| 2026-11-11 | v4.5 | §17 验收清单反向顺序一行勘误：双方均为 Y（原 X），被拒的是 A 端推的旧值（原 B 端）；与 §9.2 新者胜及第 4 轮实测一致 |
| 2026-11-11 | v4.6 | 全文复核勘误：§15 合计与 §18 全量低值 6,300→6,100（里程碑表求和）；§9.1 示例 clockOffsetMs -450→-123（与示例中 clientTime - serverTime 一致）；§7 补 IPC：reorder_list、search_tasks、due_notifications；§8 补路由：/admin、/api/token；§2.2 删 core 的 notify/（实际在 server jobs/dispatcher.rs），src-tauri 补 main.rs、android_secret.rs，server 补 admin.html；§2.1 rust-embed→include_str! 内嵌、"Webhook 备份"→"Webhook"；§9.3 修正一处 garbled 措辞 |
| 2026-11-11 | v4.7 | §18 轮次预算按 §15 里程碑表求和重定（轮 1 1,550–3,250、轮 2 800–1,600、轮 3 2,800–5,600，两处小计同步改）；§1.3 workspace_id 只改措辞（暂不设列）；§15 删 v3 对比段；§9.2 括号措辞改顺；§9.3 被拒记录权威版本随 apply 下发（v4.7，修静默留旧数据 bug，服务端改动，客户端不用改） |
