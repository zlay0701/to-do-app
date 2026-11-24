import { useEffect, useRef } from "react";
import { useTodo } from "./store";
import { Sidebar } from "./components/Sidebar";
import { TaskList } from "./components/TaskList";
import { SettingsPage } from "./components/SettingsPage";
import { SyncBadge } from "./components/SyncBadge";
import { DueBanner } from "./components/DueBanner";

export default function App() {
  const refreshAll = useTodo((s) => s.refreshAll);
  const syncNow = useTodo((s) => s.syncNow);
  const settings = useTodo((s) => s.settings);
  const startedSync = useRef(false);
  const refreshDue = useTodo((s) => s.refreshDue);
  const refreshSyncStatus = useTodo((s) => s.refreshSyncStatus);
  const view = useTodo((s) => s.view);
  const setView = useTodo((s) => s.setView);
  const drawerOpen = useTodo((s) => s.drawerOpen);
  const setDrawerOpen = useTodo((s) => s.setDrawerOpen);
  const searchQuery = useTodo((s) => s.searchQuery);
  const setSearch = useTodo((s) => s.setSearch);
  const error = useTodo((s) => s.setError);
  const errorMsg = useTodo((s) => s.error);
  const loading = useTodo((s) => s.loading);

  useEffect(() => {
    void refreshAll();
  }, [refreshAll]);

  // 启动时同步一次（仅自动同步开启且已配置时），之后前台每 15 分钟同步一次。
  useEffect(() => {
    if (startedSync.current || !settings) return;
    startedSync.current = true;
    if (settings.server.autoSync && settings.server.serverUrl && settings.hasToken) {
      void syncNow();
    }
  }, [settings, syncNow]);
  useEffect(() => {
    const id = window.setInterval(() => {
      const st = useTodo.getState();
      if (st.settings?.server.autoSync) void st.syncNow();
    }, 15 * 60_000);
    return () => window.clearInterval(id);
  }, []);

  // 每 30 秒检查一次到期提醒与同步状态（前台本地通知的心跳）。
  useEffect(() => {
    void refreshDue();
    const id = window.setInterval(() => {
      void refreshDue();
      void refreshSyncStatus();
    }, 30_000);
    return () => window.clearInterval(id);
  }, [refreshDue, refreshSyncStatus]);

  // 快捷键：N 聚焦新建任务，/ 聚焦搜索，Esc 关闭抽屉 / 返回任务视图。
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement | null;
      const typing = target && (target.tagName === "INPUT" || target.tagName === "TEXTAREA" || target.tagName === "SELECT");
      if (e.key === "Escape") {
        if (drawerOpen) setDrawerOpen(false);
        else if (view === "settings") setView("tasks");
        else if (searchQuery) void setSearch("");
        return;
      }
      if (typing) return;
      if (e.key === "n" || e.key === "N") {
        document.getElementById("new-task-input")?.focus();
      } else if (e.key === "/") {
        e.preventDefault();
        document.getElementById("search-input")?.focus();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [drawerOpen, view, searchQuery, setDrawerOpen, setView, setSearch]);

  return (
    <div className="flex h-dvh flex-col bg-gray-50 text-gray-900 md:flex-row">
      {/* ≥768px 侧边栏 */}
      <aside className="hidden w-64 shrink-0 border-r border-gray-200 bg-white md:block">
        <Sidebar />
      </aside>

      <div className="flex min-w-0 flex-1 flex-col">
        {/* 顶栏：移动端菜单 + 搜索 + 同步指示 */}
        <header className="flex items-center gap-2 border-b border-gray-200 bg-white px-3 py-2">
          <button
            className="rounded-lg px-2 py-1 text-lg hover:bg-gray-100 md:hidden"
            title="列表"
            onClick={() => setDrawerOpen(true)}
          >
            ☰
          </button>
          <input
            id="search-input"
            className="min-w-0 flex-1 rounded-lg border px-3 py-1.5 text-sm"
            placeholder="搜索任务…（按 / 快速聚焦）"
            value={searchQuery}
            onChange={(e) => void setSearch(e.target.value)}
          />
          <SyncBadge />
        </header>

        <DueBanner />

        {errorMsg && (
          <div className="flex items-center justify-between border-b border-red-200 bg-red-50 px-4 py-2 text-sm text-red-700">
            <span>{errorMsg}</span>
            <button className="rounded px-1.5 hover:bg-red-100" onClick={() => error(null)}>
              ×
            </button>
          </div>
        )}

        <main className="flex-1 overflow-y-auto pb-20 md:pb-4">
          {loading ? (
            <p className="p-4 text-sm text-gray-500">加载中…</p>
          ) : view === "settings" ? (
            <SettingsPage />
          ) : (
            <TaskList />
          )}
        </main>

        {/* <768px 底部导航 */}
        <nav className="fixed inset-x-0 bottom-0 flex border-t border-gray-200 bg-white pb-[env(safe-area-inset-bottom)] md:hidden">
          <button
            className={`flex-1 py-3 text-sm ${view === "tasks" ? "font-bold text-blue-600" : "text-gray-600"}`}
            onClick={() => setView("tasks")}
          >
            ☑ 任务
          </button>
          <button
            className={`flex-1 py-3 text-sm ${view === "settings" ? "font-bold text-blue-600" : "text-gray-600"}`}
            onClick={() => setView("settings")}
          >
            ⚙ 设置
          </button>
        </nav>
      </div>

      {/* <768px 抽屉：列表 */}
      {drawerOpen && (
        <div className="fixed inset-0 z-50 md:hidden">
          <div className="absolute inset-0 bg-black/40" onClick={() => setDrawerOpen(false)} />
          <div className="absolute inset-y-0 left-0 w-72 bg-white shadow-xl">
            <Sidebar />
          </div>
        </div>
      )}
    </div>
  );
}
