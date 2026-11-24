export function nowMs(): number {
  return Date.now();
}

export function uuid(): string {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) return crypto.randomUUID();
  return `local-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`;
}

const PRIORITY_LABEL = ["", "低", "中", "高"];
export function priorityLabel(p: number): string {
  return PRIORITY_LABEL[p] ?? "";
}

export function formatDue(dueAt: number | null): string | null {
  if (dueAt == null) return null;
  const d = new Date(dueAt);
  const today = new Date();
  const sameDay = (a: Date, b: Date) =>
    a.getFullYear() === b.getFullYear() && a.getMonth() === b.getMonth() && a.getDate() === b.getDate();
  const tomorrow = new Date(today);
  tomorrow.setDate(today.getDate() + 1);
  const hm = d.toLocaleTimeString("zh-CN", { hour: "2-digit", minute: "2-digit", hour12: false });
  if (sameDay(d, today)) return `今天 ${hm}`;
  if (sameDay(d, tomorrow)) return `明天 ${hm}`;
  return `${d.getMonth() + 1}月${d.getDate()}日 ${hm}`;
}

export function isOverdue(t: { dueAt: number | null; status: number }): boolean {
  return t.status === 0 && t.dueAt != null && t.dueAt < Date.now();
}

/** date input（yyyy-MM-dd）与 datetime-local 值 ↔ 毫秒时间戳。 */
export function dateInputToMs(value: string): number | null {
  if (!value) return null;
  const ms = new Date(value).getTime();
  return Number.isNaN(ms) ? null : ms;
}

export function msToDateInput(ms: number | null): string {
  if (ms == null) return "";
  const d = new Date(ms);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`;
}
