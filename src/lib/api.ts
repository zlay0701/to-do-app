// IPC 抽象：Tauri 环境调 Rust 命令；浏览器 `pnpm dev` 时用 localStorage 实现
// 同一套语义（收件箱、updatedAt/dirty/origin、软删除），保证浏览器里也能记待办。
import { invoke, isTauri } from "@tauri-apps/api/core";
import type {
  AppSettings,
  Counts,
  ListCount,
  SyncStatus,
  Task,
  TaskPatch,
  TestResult,
  TodoList,
} from "./types";
import { nowMs, uuid } from "./utils";

export interface TodoApi {
  listLists(): Promise<TodoList[]>;
  createList(name: string, color: string): Promise<TodoList>;
  renameList(id: string, name: string): Promise<TodoList>;
  deleteList(id: string): Promise<void>;
  reorderList(id: string, sortOrder: number): Promise<void>;
  listTasks(listId: string): Promise<Task[]>;
  createTask(
    listId: string,
    input: { title: string; notes?: string; priority?: number; dueAt?: number | null },
  ): Promise<Task>;
  updateTask(id: string, patch: TaskPatch): Promise<Task>;
  toggleTask(id: string): Promise<Task>;
  deleteTask(id: string): Promise<void>;
  reorderTask(id: string, sortOrder: number): Promise<void>;
  searchTasks(query: string): Promise<Task[]>;
  counts(): Promise<Counts>;
  getSettings(): Promise<AppSettings>;
  saveSettings(scope: "server" | "local_notify", payload: Record<string, unknown>): Promise<void>;
  testServer(): Promise<TestResult>;
  syncNow(): Promise<string>;
  syncStatus(): Promise<SyncStatus>;
  resetSync(): Promise<string>;
  dueNotifications(): Promise<Task[]>;
}

class TauriApi implements TodoApi {
  listLists = () => invoke<TodoList[]>("list_lists");
  createList = (name: string, color: string) => invoke<TodoList>("create_list", { name, color });
  renameList = (id: string, name: string) => invoke<TodoList>("rename_list", { id, name });
  deleteList = (id: string) => invoke<void>("delete_list", { id });
  reorderList = (id: string, sortOrder: number) =>
    invoke<void>("reorder_list", { id, sortOrder });
  listTasks = (listId: string) => invoke<Task[]>("list_tasks", { listId });
  createTask = (
    listId: string,
    input: { title: string; notes?: string; priority?: number; dueAt?: number | null },
  ) =>
    invoke<Task>("create_task", {
      listId,
      title: input.title,
      notes: input.notes ?? "",
      priority: input.priority ?? 0,
      dueAt: input.dueAt ?? null,
    });
  updateTask = (id: string, patch: TaskPatch) => invoke<Task>("update_task", { id, patch });
  toggleTask = (id: string) => invoke<Task>("toggle_task", { id });
  deleteTask = (id: string) => invoke<void>("delete_task", { id });
  reorderTask = (id: string, sortOrder: number) =>
    invoke<void>("reorder_task", { id, sortOrder });
  searchTasks = (query: string) => invoke<Task[]>("search_tasks", { query });
  counts = () => invoke<Counts>("counts");
  getSettings = () => invoke<AppSettings>("get_settings");
  saveSettings = (scope: "server" | "local_notify", payload: Record<string, unknown>) =>
    invoke<void>("save_settings", { scope, payload });
  testServer = () => invoke<TestResult>("test_server");
  syncNow = () => invoke<string>("sync_now");
  syncStatus = () => invoke<SyncStatus>("sync_status");
  resetSync = () => invoke<string>("reset_sync");
  dueNotifications = () => invoke<Task[]>("due_notifications");
}

// -- 浏览器 fallback：localStorage 单机实现，与 Rust repo 同语义 ----------------

const LS_KEY = "todo-local-v1";
const INBOX_ID = "inbox";

interface LocalDb {
  deviceId: string;
  lists: TodoList[];
  tasks: Task[];
  settings: AppSettings;
}

function defaultSettings(): AppSettings {
  return {
    server: { serverUrl: "", syncIntervalMin: 15, autoSync: true },
    localNotify: { enabled: true, beforeMin: 0 },
    hasToken: false,
  };
}

class LocalApi implements TodoApi {
  private db: LocalDb;

  constructor() {
    const raw = localStorage.getItem(LS_KEY);
    if (raw) {
      this.db = JSON.parse(raw) as LocalDb;
    } else {
      const now = nowMs();
      this.db = {
        deviceId: uuid(),
        lists: [
          {
            id: INBOX_ID,
            name: "收件箱",
            color: "#378ADD",
            sortOrder: 0,
            deletedAt: null,
            updatedAt: now,
            origin: "",
            createdAt: now,
          },
        ],
        tasks: [],
        settings: defaultSettings(),
      };
      this.save();
    }
  }

  private save() {
    localStorage.setItem(LS_KEY, JSON.stringify(this.db));
  }

  private touchList(l: TodoList): TodoList {
    return { ...l, updatedAt: nowMs(), origin: this.db.deviceId };
  }

  private touchTask(t: Task): Task {
    return { ...t, updatedAt: nowMs(), origin: this.db.deviceId };
  }

  private liveLists() {
    return this.db.lists.filter((l) => l.deletedAt == null);
  }

  async listLists() {
    return [...this.liveLists()].sort((a, b) => a.sortOrder - b.sortOrder);
  }

  async createList(name: string, color: string) {
    const now = nowMs();
    const list: TodoList = {
      id: uuid(),
      name: name.trim(),
      color,
      sortOrder: Math.max(-1, ...this.db.lists.map((l) => l.sortOrder)) + 1,
      deletedAt: null,
      updatedAt: now,
      origin: this.db.deviceId,
      createdAt: now,
    };
    this.db.lists.push(list);
    this.save();
    return list;
  }

  async renameList(id: string, name: string) {
    const list = this.db.lists.find((l) => l.id === id);
    if (!list) throw new Error("列表不存在");
    const updated = this.touchList({ ...list, name });
    Object.assign(list, updated);
    this.save();
    return updated;
  }

  async deleteList(id: string) {
    if (id === INBOX_ID) throw new Error("收件箱不能删除");
    const now = nowMs();
    const list = this.db.lists.find((l) => l.id === id);
    if (!list) return;
    Object.assign(list, this.touchList({ ...list, deletedAt: now }));
    for (const t of this.db.tasks.filter((t) => t.listId === id && t.deletedAt == null)) {
      Object.assign(t, this.touchTask({ ...t, deletedAt: now }));
    }
    this.save();
  }

  async reorderList(id: string, sortOrder: number) {
    const list = this.db.lists.find((l) => l.id === id);
    if (list) {
      Object.assign(list, this.touchList({ ...list, sortOrder }));
      this.save();
    }
  }

  async listTasks(listId: string) {
    return this.db.tasks
      .filter((t) => t.listId === listId && t.deletedAt == null)
      .sort((a, b) => a.status - b.status || a.sortOrder - b.sortOrder);
  }

  async createTask(
    listId: string,
    input: { title: string; notes?: string; priority?: number; dueAt?: number | null },
  ) {
    if (!input.title.trim()) throw new Error("任务标题不能为空");
    const now = nowMs();
    const siblings = this.db.tasks.filter((t) => t.listId === listId && t.deletedAt == null);
    const task: Task = {
      id: uuid(),
      listId,
      title: input.title.trim(),
      notes: input.notes ?? "",
      status: 0,
      priority: input.priority ?? 0,
      dueAt: input.dueAt ?? null,
      completedAt: null,
      sortOrder: Math.max(0, ...siblings.map((t) => t.sortOrder)) + 1,
      deletedAt: null,
      updatedAt: now,
      origin: this.db.deviceId,
      createdAt: now,
    };
    this.db.tasks.push(task);
    this.save();
    return task;
  }

  async updateTask(id: string, patch: TaskPatch) {
    const task = this.db.tasks.find((t) => t.id === id);
    if (!task) throw new Error("任务不存在");
    const next: Task = { ...task };
    if (patch.title !== undefined) next.title = patch.title;
    if (patch.notes !== undefined) next.notes = patch.notes;
    if (patch.priority !== undefined) next.priority = patch.priority;
    if (patch.dueAt !== undefined) next.dueAt = patch.dueAt;
    if (patch.listId !== undefined) next.listId = patch.listId;
    const updated = this.touchTask(next);
    Object.assign(task, updated);
    this.save();
    return updated;
  }

  async toggleTask(id: string) {
    const task = this.db.tasks.find((t) => t.id === id);
    if (!task) throw new Error("任务不存在");
    const done = task.status === 1;
    const updated = this.touchTask({
      ...task,
      status: done ? 0 : 1,
      completedAt: done ? null : nowMs(),
    });
    Object.assign(task, updated);
    this.save();
    return updated;
  }

  async deleteTask(id: string) {
    const task = this.db.tasks.find((t) => t.id === id);
    if (task && task.deletedAt == null) {
      Object.assign(task, this.touchTask({ ...task, deletedAt: nowMs() }));
      this.save();
    }
  }

  async reorderTask(id: string, sortOrder: number) {
    const task = this.db.tasks.find((t) => t.id === id);
    if (task) {
      Object.assign(task, this.touchTask({ ...task, sortOrder }));
      this.save();
    }
  }

  async searchTasks(query: string) {
    const q = query.trim().toLowerCase();
    if (!q) return [];
    return this.db.tasks
      .filter(
        (t) =>
          t.deletedAt == null &&
          (t.title.toLowerCase().includes(q) || t.notes.toLowerCase().includes(q)),
      )
      .sort((a, b) => b.updatedAt - a.updatedAt)
      .slice(0, 50);
  }

  async counts(): Promise<Counts> {
    const counts: ListCount[] = this.liveLists().map((l) => {
      const ts = this.db.tasks.filter((t) => t.listId === l.id && t.deletedAt == null);
      return {
        listId: l.id,
        active: ts.filter((t) => t.status === 0).length,
        done: ts.filter((t) => t.status === 1).length,
      };
    });
    // 浏览器单机模式：全部记录都未同步过
    return { counts, pendingDirty: this.db.lists.length + this.db.tasks.length };
  }

  async getSettings() {
    return this.db.settings;
  }

  async saveSettings(scope: "server" | "local_notify", payload: Record<string, unknown>) {
    if (scope === "server") {
      if (typeof payload.serverUrl === "string") this.db.settings.server.serverUrl = payload.serverUrl;
      if (typeof payload.syncIntervalMin === "number")
        this.db.settings.server.syncIntervalMin = payload.syncIntervalMin;
      if (typeof payload.autoSync === "boolean") this.db.settings.server.autoSync = payload.autoSync;
      if (typeof payload.token === "string" && payload.token.length > 0) {
        this.db.settings.hasToken = true;
      }
    } else {
      if (typeof payload.enabled === "boolean") this.db.settings.localNotify.enabled = payload.enabled;
      if (typeof payload.beforeMin === "number") this.db.settings.localNotify.beforeMin = payload.beforeMin;
    }
    this.save();
  }

  async testServer(): Promise<TestResult> {
    const url = this.db.settings.server.serverUrl.trim();
    if (!url) return { ok: false, status: null, message: "请先填写服务端地址" };
    try {
      // 浏览器 fallback 不保存令牌原文，只存 hasToken，因此这里不带鉴权头；
      // 401 只说明服务端要求令牌，完整校验请在桌面端做。
      const res = await fetch(`${url.replace(/\/$/, "")}/api/v1/ping`);
      return {
        ok: res.ok,
        status: res.status,
        message: res.ok
          ? "连接成功"
          : res.status === 401
            ? "服务端要求同步令牌（浏览器模式下不保存令牌，请在桌面端测试）"
            : `服务端返回 HTTP ${res.status}`,
      };
    } catch {
      return { ok: false, status: null, message: "无法连接服务端，请检查地址与网络" };
    }
  }

  async syncNow(): Promise<string> {
    throw new Error("同步引擎在 M5 实现（第 3 轮）；本轮为单机模式");
  }

  async resetSync(): Promise<string> {
    return "浏览器单机模式没有同步游标，无需重置";
  }

  async syncStatus(): Promise<SyncStatus> {
    const c = await this.counts();
    return {
      state: "idle",
      lastSyncAt: null,
      pendingCount: c.pendingDirty,
      lastError: null,
      clockOffsetMs: null,
    };
  }

  async dueNotifications() {
    const { enabled, beforeMin } = this.db.settings.localNotify;
    if (!enabled) return [];
    const now = nowMs();
    return this.db.tasks.filter(
      (t) =>
        t.status === 0 &&
        t.deletedAt == null &&
        t.dueAt != null &&
        t.dueAt <= now + beforeMin * 60_000 &&
        t.dueAt >= now - 24 * 60 * 60_000,
    );
  }
}

export const api: TodoApi = isTauri() ? new TauriApi() : new LocalApi();

export const isTauriEnv = isTauri();
