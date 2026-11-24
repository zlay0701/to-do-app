import { create } from "zustand";
import { api } from "./lib/api";
import type { AppSettings, Counts, SyncStatus, Task, TodoList } from "./lib/types";

export type View = "tasks" | "settings";

interface TodoState {
  lists: TodoList[];
  tasks: Task[];
  counts: Counts | null;
  selectedListId: string | null;
  searchQuery: string;
  searchResults: Task[] | null;
  view: View;
  drawerOpen: boolean;
  settings: AppSettings | null;
  syncStatus: SyncStatus | null;
  dueTasks: Task[];
  dismissedDue: string[];
  error: string | null;
  loading: boolean;

  refreshAll: () => Promise<void>;
  refreshTasks: () => Promise<void>;
  selectList: (id: string) => void;
  setSearch: (q: string) => Promise<void>;
  setView: (v: View) => void;
  setDrawerOpen: (open: boolean) => void;
  createList: (name: string, color?: string) => Promise<void>;
  renameList: (id: string, name: string) => Promise<void>;
  deleteList: (id: string) => Promise<void>;
  createTask: (input: {
    title: string;
    notes?: string;
    priority?: number;
    dueAt?: number | null;
  }) => Promise<void>;
  updateTask: (id: string, patch: Partial<Task>) => Promise<void>;
  toggleTask: (id: string) => Promise<void>;
  deleteTask: (id: string) => Promise<void>;
  reorderTask: (id: string, newIndex: number) => Promise<void>;
  reorderList: (id: string, newIndex: number) => Promise<void>;
  refreshSyncStatus: () => Promise<void>;
  syncNow: () => Promise<void>;
  resetSync: () => Promise<string>;
  refreshSettings: () => Promise<void>;
  refreshDue: () => Promise<void>;
  dismissDue: (id: string) => void;
  setError: (msg: string | null) => void;
}

const LIST_COLORS = ["#378ADD", "#E5484D", "#46A758", "#F5A524", "#8E4EC6", "#12A594"];

// 变更后 5 分钟防抖同步（SPEC §5.1）：每次变更重置计时器。
let debounceTimer: number | undefined;
function scheduleDebouncedSync() {
  // 防抖同步是自动同步，只在 autoSync 开启时生效；手动同步不受此限。
  if (!useTodo.getState().settings?.server.autoSync) return;
  window.clearTimeout(debounceTimer);
  debounceTimer = window.setTimeout(() => {
    void useTodo.getState().syncNow();
  }, 5 * 60_000);
}

export const useTodo = create<TodoState>()((set, get) => ({
  lists: [],
  tasks: [],
  counts: null,
  selectedListId: null,
  searchQuery: "",
  searchResults: null,
  view: "tasks",
  drawerOpen: false,
  settings: null,
  syncStatus: null,
  dueTasks: [],
  dismissedDue: [],
  error: null,
  loading: true,

  refreshAll: async () => {
    set({ loading: true, error: null });
    try {
      const [lists, counts, settings, syncStatus] = await Promise.all([
        api.listLists(),
        api.counts(),
        api.getSettings(),
        api.syncStatus(),
      ]);
      const selected = get().selectedListId ?? lists[0]?.id ?? null;
      const tasks = selected ? await api.listTasks(selected) : [];
      set({ lists, counts, settings, syncStatus, selectedListId: selected, tasks, loading: false });
    } catch (e) {
      set({ error: e instanceof Error ? e.message : String(e), loading: false });
    }
  },

  refreshTasks: async () => {
    const { selectedListId, searchQuery } = get();
    try {
      const [counts, syncStatus] = await Promise.all([api.counts(), api.syncStatus()]);
      if (searchQuery.trim()) {
        const searchResults = await api.searchTasks(searchQuery);
        set({ counts, syncStatus, searchResults });
      } else if (selectedListId) {
        const tasks = await api.listTasks(selectedListId);
        set({ tasks, counts, syncStatus, searchResults: null });
      } else {
        set({ counts, syncStatus });
      }
    } catch (e) {
      set({ error: e instanceof Error ? e.message : String(e) });
    }
  },

  selectList: (id) => {
    set({ selectedListId: id, searchQuery: "", searchResults: null, drawerOpen: false, view: "tasks" });
    void get().refreshTasks();
  },

  setSearch: async (q) => {
    set({ searchQuery: q });
    if (!q.trim()) {
      set({ searchResults: null });
      await get().refreshTasks();
      return;
    }
    try {
      const searchResults = await api.searchTasks(q);
      set({ searchResults });
    } catch (e) {
      set({ error: e instanceof Error ? e.message : String(e) });
    }
  },

  setView: (view) => set({ view, drawerOpen: false }),
  setDrawerOpen: (open) => set({ drawerOpen: open }),

  createList: async (name, color) => {
    const c = color ?? LIST_COLORS[get().lists.length % LIST_COLORS.length];
    const list = await api.createList(name, c);
    set({ lists: [...get().lists, list], selectedListId: list.id });
    await get().refreshTasks();
    scheduleDebouncedSync();
  },

  renameList: async (id, name) => {
    const updated = await api.renameList(id, name);
    set({ lists: get().lists.map((l) => (l.id === id ? updated : l)) });
    scheduleDebouncedSync();
  },

  deleteList: async (id) => {
    await api.deleteList(id);
    const lists = get().lists.filter((l) => l.id !== id);
    set({ lists, selectedListId: lists[0]?.id ?? null });
    await get().refreshTasks();
    scheduleDebouncedSync();
  },

  createTask: async (input) => {
    const { selectedListId } = get();
    if (!selectedListId) return;
    await api.createTask(selectedListId, input);
    await get().refreshTasks();
    scheduleDebouncedSync();
  },

  updateTask: async (id, patch) => {
    await api.updateTask(id, {
      title: patch.title,
      notes: patch.notes,
      priority: patch.priority,
      dueAt: patch.dueAt,
      listId: patch.listId,
    });
    await get().refreshTasks();
    scheduleDebouncedSync();
  },

  toggleTask: async (id) => {
    await api.toggleTask(id);
    await get().refreshTasks();
    scheduleDebouncedSync();
  },

  deleteTask: async (id) => {
    await api.deleteTask(id);
    await get().refreshTasks();
    scheduleDebouncedSync();
  },

  // 拖拽到新位置：取相邻两条的 sortOrder 中点，落在首尾则 ±1。
  reorderTask: async (id, newIndex) => {
    const active = get().tasks.filter((t) => t.status === 0);
    const without = active.filter((t) => t.id !== id);
    const clamped = Math.max(0, Math.min(newIndex, without.length));
    const prev = without[clamped - 1];
    const next = without[clamped];
    let sortOrder: number;
    if (prev && next) sortOrder = (prev.sortOrder + next.sortOrder) / 2;
    else if (prev) sortOrder = prev.sortOrder + 1;
    else if (next) sortOrder = next.sortOrder - 1;
    else sortOrder = 1;
    await api.reorderTask(id, sortOrder);
    await get().refreshTasks();
    scheduleDebouncedSync();
  },

  // 列表拖拽到新位置：列表 sortOrder 是整数、没有中点可取，改为按新顺序重排号，
  // 只写 sortOrder 真变了的列表（每条写入口都会打 dirty，同步时会推上去）。
  reorderList: async (id, newIndex) => {
    const lists = get().lists;
    const moved = lists.find((l) => l.id === id);
    if (!moved) return;
    const without = lists.filter((l) => l.id !== id);
    const clamped = Math.max(0, Math.min(newIndex, without.length));
    const ordered = [...without.slice(0, clamped), moved, ...without.slice(clamped)];
    for (let i = 0; i < ordered.length; i++) {
      if (ordered[i].sortOrder !== i) await api.reorderList(ordered[i].id, i);
    }
    await get().refreshAll();
    scheduleDebouncedSync();
  },

  syncNow: async () => {
    const settings = get().settings;
    if (!settings?.server.serverUrl || !settings.hasToken) return;
    try {
      await api.syncNow();
    } catch {
      // 离线静默失败、dirty 累积，恢复后自动补传（§5.1）；不弹错
    }
    await get().refreshTasks();
    await get().refreshSyncStatus();
  },

  refreshSyncStatus: async () => {
    try {
      set({ syncStatus: await api.syncStatus() });
    } catch {
      // 同步状态读失败不阻塞主界面
    }
  },

  // v4.4：重置同步后立即同步一次，让全量重拉马上发生而不是等下个周期。
  resetSync: async () => {
    const message = await api.resetSync();
    await get().refreshSyncStatus();
    await get().syncNow();
    return message;
  },

  refreshSettings: async () => {
    set({ settings: await api.getSettings() });
  },

  refreshDue: async () => {
    try {
      const due = await api.dueNotifications();
      const dismissed = get().dismissedDue;
      set({ dueTasks: due.filter((t) => !dismissed.includes(t.id)) });
    } catch {
      // 提醒读取失败不阻塞主界面
    }
  },

  dismissDue: (id) =>
    set({ dismissedDue: [...get().dismissedDue, id], dueTasks: get().dueTasks.filter((t) => t.id !== id) }),

  setError: (error) => set({ error }),
}));
