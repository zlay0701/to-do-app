import { useTodo } from "../store";

/** 状态区同步指示（SPEC §6）：点按立即同步；时钟漂移 > 120s 时黄色告警（§9.4）。 */
export function SyncBadge() {
  const syncStatus = useTodo((s) => s.syncStatus);
  const syncNow = useTodo((s) => s.syncNow);

  if (!syncStatus) return null;
  const pending = syncStatus.pendingCount;
  const clockSkew =
    syncStatus.clockOffsetMs != null && Math.abs(syncStatus.clockOffsetMs) > 120_000;
  const label =
    syncStatus.state === "syncing"
      ? "同步中…"
      : clockSkew
        ? "设备时间不准，可能导致旧数据覆盖新数据"
        : syncStatus.state === "error"
          ? `同步失败：${syncStatus.lastError ?? "未知错误"}`
          : syncStatus.state === "conflict"
            ? "同步有冲突（以新者为准）"
            : pending > 0
              ? `本地修改 ${pending} 条待同步`
              : "已同步";
  const cls =
    syncStatus.state === "error"
      ? "bg-red-100 text-red-700"
      : clockSkew || syncStatus.state === "conflict" || pending > 0
        ? "bg-amber-100 text-amber-800"
        : "bg-green-100 text-green-700";
  const title = syncStatus.lastSyncAt
    ? `上次同步：${new Date(syncStatus.lastSyncAt).toLocaleString("zh-CN")}${
        clockSkew ? `；时钟漂移 ${syncStatus.clockOffsetMs}ms` : ""
      }`
    : "尚未同步过（服务端未配置时为单机模式）";

  return (
    <button
      className={`rounded-full px-2.5 py-1 text-xs ${cls}`}
      title={title}
      onClick={() => void syncNow()}
    >
      {label}
    </button>
  );
}
