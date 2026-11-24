import { useState } from "react";
import { useTodo } from "../store";
import type { Task } from "../lib/types";
import { dateInputToMs, formatDue, isOverdue, msToDateInput, priorityLabel } from "../lib/utils";

function TaskRow({
  task,
  draggable,
  onDragStart,
  onDragOver,
  onDrop,
}: {
  task: Task;
  draggable: boolean;
  onDragStart: () => void;
  onDragOver: (e: React.DragEvent) => void;
  onDrop: () => void;
}) {
  const toggleTask = useTodo((s) => s.toggleTask);
  const deleteTask = useTodo((s) => s.deleteTask);
  const updateTask = useTodo((s) => s.updateTask);
  const setError = useTodo((s) => s.setError);

  const [editing, setEditing] = useState(false);
  const [title, setTitle] = useState(task.title);
  const [notes, setNotes] = useState(task.notes);
  const [dueInput, setDueInput] = useState(msToDateInput(task.dueAt));
  const [priority, setPriority] = useState(task.priority);

  const save = async () => {
    setEditing(false);
    try {
      await updateTask(task.id, {
        title: title.trim() || task.title,
        notes,
        dueAt: dateInputToMs(dueInput),
        priority,
      });
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const due = formatDue(task.dueAt);
  const overdue = isOverdue(task);

  return (
    <li
      draggable={draggable}
      onDragStart={onDragStart}
      onDragOver={onDragOver}
      onDrop={onDrop}
      className={`flex items-start gap-2 rounded-lg border px-3 py-2.5 ${
        task.status === 1 ? "border-gray-100 bg-gray-50 opacity-60" : "border-gray-200 bg-white"
      } ${draggable ? "cursor-grab active:cursor-grabbing" : ""}`}
    >
      <button
        className={`mt-0.5 flex h-5 w-5 shrink-0 items-center justify-center rounded-full border-2 text-xs text-white ${
          task.status === 1 ? "border-green-600 bg-green-600" : "border-gray-300 hover:border-green-600"
        }`}
        title={task.status === 1 ? "标记为未完成" : "标记为完成"}
        onClick={() => void toggleTask(task.id).catch((e: unknown) => setError(String(e)))}
      >
        {task.status === 1 ? "✓" : ""}
      </button>
      <div className="min-w-0 flex-1">
        {editing ? (
          <div className="space-y-2">
            <input
              autoFocus
              className="w-full rounded border px-2 py-1 text-sm"
              value={title}
              onChange={(e) => setTitle(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") void save();
                if (e.key === "Escape") setEditing(false);
              }}
            />
            <input
              className="w-full rounded border px-2 py-1 text-sm"
              placeholder="备注…"
              value={notes}
              onChange={(e) => setNotes(e.target.value)}
            />
            <div className="flex gap-2">
              <input
                type="datetime-local"
                className="min-w-0 flex-1 rounded border px-2 py-1 text-sm"
                value={dueInput}
                onChange={(e) => setDueInput(e.target.value)}
              />
              <select
                className="rounded border px-2 py-1 text-sm"
                value={priority}
                onChange={(e) => setPriority(Number(e.target.value))}
              >
                <option value={0}>无优先级</option>
                <option value={1}>低</option>
                <option value={2}>中</option>
                <option value={3}>高</option>
              </select>
              <button
                className="rounded bg-blue-600 px-3 text-sm text-white"
                onClick={() => void save()}
              >
                保存
              </button>
            </div>
          </div>
        ) : (
          <button className="block w-full text-left" onClick={() => setEditing(true)} title="点击编辑">
            <span className={`text-sm ${task.status === 1 ? "line-through" : ""}`}>{task.title}</span>
            <span className="mt-0.5 flex flex-wrap gap-2 text-xs">
              {task.priority > 0 && (
                <span
                  className={`rounded px-1.5 py-0.5 ${
                    task.priority === 3
                      ? "bg-red-100 text-red-700"
                      : task.priority === 2
                        ? "bg-amber-100 text-amber-800"
                        : "bg-gray-100 text-gray-600"
                  }`}
                >
                  {priorityLabel(task.priority)}
                </span>
              )}
              {due && (
                <span className={overdue ? "font-medium text-red-600" : "text-gray-500"}>
                  {overdue ? "已过期 · " : ""}📅 {due}
                </span>
              )}
            </span>
            {task.notes && <span className="mt-0.5 block truncate text-xs text-gray-500">{task.notes}</span>}
          </button>
        )}
      </div>
      <button
        className="shrink-0 rounded px-1.5 text-gray-400 hover:bg-red-50 hover:text-red-600"
        title="删除任务"
        onClick={() => {
          if (window.confirm(`删除任务「${task.title}」？`)) {
            void deleteTask(task.id).catch((e: unknown) => setError(String(e)));
          }
        }}
      >
        ×
      </button>
    </li>
  );
}

export function TaskList() {
  const tasks = useTodo((s) => s.tasks);
  const searchQuery = useTodo((s) => s.searchQuery);
  const searchResults = useTodo((s) => s.searchResults);
  const selectedList = useTodo((s) => s.lists.find((l) => l.id === s.selectedListId));
  const createTask = useTodo((s) => s.createTask);
  const reorderTask = useTodo((s) => s.reorderTask);
  const setError = useTodo((s) => s.setError);

  const [title, setTitle] = useState("");
  const [dueInput, setDueInput] = useState("");
  const [priority, setPriority] = useState(0);
  const [showDone, setShowDone] = useState(false);
  const [dragId, setDragId] = useState<string | null>(null);
  const [dropIndex, setDropIndex] = useState<number | null>(null);

  const shown = searchResults ?? tasks;
  const active = shown.filter((t) => t.status === 0);
  const done = shown.filter((t) => t.status === 1);
  const searching = searchQuery.trim().length > 0;

  const submit = async () => {
    const t = title.trim();
    if (!t) return;
    try {
      await createTask({ title: t, priority, dueAt: dateInputToMs(dueInput) });
      setTitle("");
      setDueInput("");
      setPriority(0);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const handleDrop = (index: number) => {
    if (dragId == null) return;
    void reorderTask(dragId, index).catch((e: unknown) => setError(String(e)));
    setDragId(null);
    setDropIndex(null);
  };

  return (
    <div className="mx-auto w-full max-w-2xl px-4 py-4">
      <h1 className="mb-3 text-xl font-bold">
        {searching ? `搜索“${searchQuery}”` : (selectedList?.name ?? "待办")}
      </h1>

      {!searching && (
        <div className="mb-4 flex flex-col gap-2 rounded-xl border border-gray-200 bg-white p-3 shadow-sm sm:flex-row">
          <input
            id="new-task-input"
            className="min-w-0 flex-1 rounded-lg border px-3 py-2 text-sm"
            placeholder="添加任务…（按 N 快速聚焦）"
            value={title}
            onChange={(e) => setTitle(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") void submit();
            }}
          />
          <div className="flex gap-2">
            <input
              type="datetime-local"
              className="min-w-0 flex-1 rounded-lg border px-2 py-2 text-sm sm:w-44 sm:flex-none"
              value={dueInput}
              onChange={(e) => setDueInput(e.target.value)}
              title="截止时间"
            />
            <select
              className="rounded-lg border px-2 py-2 text-sm"
              value={priority}
              onChange={(e) => setPriority(Number(e.target.value))}
            >
              <option value={0}>优先级</option>
              <option value={1}>低</option>
              <option value={2}>中</option>
              <option value={3}>高</option>
            </select>
            <button
              className="shrink-0 rounded-lg bg-blue-600 px-4 text-sm text-white hover:bg-blue-700"
              onClick={() => void submit()}
            >
              添加
            </button>
          </div>
        </div>
      )}

      {shown.length === 0 ? (
        <div className="rounded-xl border border-dashed border-gray-300 px-4 py-12 text-center">
          <div className="text-3xl">✓</div>
          <p className="mt-2 font-medium text-gray-700">
            {searching ? "没有找到匹配的任务" : "还没有任务"}
          </p>
          <p className="mt-1 text-sm text-gray-500">
            {searching ? "换个关键词试试" : "在上方输入框添加你的第一项任务吧"}
          </p>
        </div>
      ) : (
        <>
          <ul className="space-y-2">
            {active.map((task, i) => (
              <TaskRow
                key={task.id}
                task={task}
                draggable={!searching}
                onDragStart={() => setDragId(task.id)}
                onDragOver={(e) => {
                  e.preventDefault();
                  setDropIndex(i);
                }}
                onDrop={() => handleDrop(dropIndex ?? i)}
              />
            ))}
          </ul>
          {active.length === 0 && done.length > 0 && (
            <p className="mt-2 text-sm text-gray-500">全部任务都已完成 🎉</p>
          )}
          {done.length > 0 && (
            <div className="mt-4">
              <button
                className="text-sm text-gray-500 hover:text-gray-800"
                onClick={() => setShowDone((v) => !v)}
              >
                {showDone ? "▾" : "▸"} 已完成（{done.length}）
              </button>
              {showDone && (
                <ul className="mt-2 space-y-2">
                  {done.map((task) => (
                    <TaskRow
                      key={task.id}
                      task={task}
                      draggable={false}
                      onDragStart={() => {}}
                      onDragOver={() => {}}
                      onDrop={() => {}}
                    />
                  ))}
                </ul>
              )}
            </div>
          )}
          {!searching && active.length > 0 && (
            <p className="mt-3 text-xs text-gray-400">拖动任务可调整顺序；点击任务可编辑标题、备注、截止时间与优先级。</p>
          )}
        </>
      )}
    </div>
  );
}
