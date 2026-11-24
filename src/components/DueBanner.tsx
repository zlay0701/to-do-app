import { useTodo } from "../store";
import { formatDue } from "../lib/utils";

/** 前台到期提醒横幅（M4）：应用前台时到期按本机偏好提醒。 */
export function DueBanner() {
  const dueTasks = useTodo((s) => s.dueTasks);
  const dismissDue = useTodo((s) => s.dismissDue);
  const selectList = useTodo((s) => s.selectList);

  if (dueTasks.length === 0) return null;

  const requestSystemNotify = () => {
    if ("Notification" in window && Notification.permission === "default") {
      void Notification.requestPermission();
    }
  };

  return (
    <div className="border-b border-amber-200 bg-amber-50 px-4 py-2">
      <div className="mb-1 flex items-center justify-between">
        <span className="text-sm font-medium text-amber-800">
          ⏰ {dueTasks.length} 项任务已到期
        </span>
        {"Notification" in window && Notification.permission === "default" && (
          <button className="text-xs text-amber-700 underline" onClick={requestSystemNotify}>
            开启系统提醒
          </button>
        )}
      </div>
      <ul className="space-y-1">
        {dueTasks.map((t) => (
          <li key={t.id} className="flex items-center gap-2 text-sm">
            <button
              className="min-w-0 flex-1 truncate text-left text-amber-900 underline-offset-2 hover:underline"
              onClick={() => selectList(t.listId)}
            >
              {t.title}
              {t.dueAt != null && (
                <span className="ml-2 text-xs text-amber-700">{formatDue(t.dueAt)}</span>
              )}
            </button>
            <button
              className="shrink-0 rounded px-1.5 text-xs text-amber-700 hover:bg-amber-100"
              onClick={() => dismissDue(t.id)}
            >
              忽略
            </button>
          </li>
        ))}
      </ul>
    </div>
  );
}
