// 与 crates/core/src/models.rs 的 serde 形状一一对应（camelCase）。
export interface TodoList {
  id: string;
  name: string;
  color: string;
  sortOrder: number;
  deletedAt: number | null;
  updatedAt: number;
  origin: string;
  createdAt: number;
}

export interface Task {
  id: string;
  listId: string;
  title: string;
  notes: string;
  status: number; // 0 未完成，1 已完成
  priority: number; // 0 无，1 低，2 中，3 高
  dueAt: number | null;
  completedAt: number | null;
  sortOrder: number;
  deletedAt: number | null;
  updatedAt: number;
  origin: string;
  createdAt: number;
}

export interface ListCount {
  listId: string;
  active: number;
  done: number;
}

export interface Counts {
  counts: ListCount[];
  pendingDirty: number;
}

export interface ServerSettings {
  serverUrl: string;
  syncIntervalMin: number;
  autoSync: boolean;
}

export interface LocalNotifySettings {
  enabled: boolean;
  beforeMin: number;
}

export interface AppSettings {
  server: ServerSettings;
  localNotify: LocalNotifySettings;
  hasToken: boolean;
}

export interface SyncStatus {
  state: string; // idle | syncing | ok | conflict | error（本轮单机只有 idle/syncing/error）
  lastSyncAt: number | null;
  pendingCount: number;
  lastError: string | null;
  clockOffsetMs: number | null;
}

export interface TestResult {
  ok: boolean;
  status: number | null;
  message: string;
}

export interface TaskPatch {
  title?: string;
  notes?: string;
  priority?: number;
  dueAt?: number | null;
  listId?: string;
}
