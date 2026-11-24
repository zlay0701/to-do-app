import { useState } from "react";
import { useTodo } from "../store";

export function Sidebar() {
  const lists = useTodo((s) => s.lists);
  const counts = useTodo((s) => s.counts);
  const selectedListId = useTodo((s) => s.selectedListId);
  const selectList = useTodo((s) => s.selectList);
  const createList = useTodo((s) => s.createList);
  const renameList = useTodo((s) => s.renameList);
  const deleteList = useTodo((s) => s.deleteList);
  const reorderList = useTodo((s) => s.reorderList);
  const setError = useTodo((s) => s.setError);

  const [newName, setNewName] = useState("");
  const [editingId, setEditingId] = useState<string | null>(null);
  const [editingName, setEditingName] = useState("");
  // 列表拖拽：与 TaskList 同一套 HTML5 拖拽写法（dragId + dropIndex）。
  const [dragId, setDragId] = useState<string | null>(null);
  const [dropIndex, setDropIndex] = useState<number | null>(null);

  const handleDrop = (index: number) => {
    if (dragId == null) return;
    void reorderList(dragId, dropIndex ?? index).catch((e: unknown) =>
      setError(e instanceof Error ? e.message : String(e)),
    );
    setDragId(null);
    setDropIndex(null);
  };

  const countFor = (id: string) => counts?.counts.find((c) => c.listId === id)?.active ?? 0;

  const submitNew = async () => {
    const name = newName.trim();
    if (!name) return;
    try {
      await createList(name);
      setNewName("");
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const submitRename = async (id: string) => {
    const name = editingName.trim();
    setEditingId(null);
    if (!name) return;
    try {
      await renameList(id, name);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  return (
    <div className="flex h-full flex-col">
      <div className="px-4 pb-2 pt-4 text-lg font-bold">待办</div>
      <nav className="flex-1 overflow-y-auto px-2">
        {lists.map((list, i) => (
          <div
            key={list.id}
            draggable
            onDragStart={() => setDragId(list.id)}
            onDragOver={(e) => {
              e.preventDefault();
              setDropIndex(i);
            }}
            onDrop={() => handleDrop(i)}
            onDragEnd={() => {
              setDragId(null);
              setDropIndex(null);
            }}
            className={`group flex items-center gap-2 rounded-lg px-2 py-2 ${
              dropIndex === i && dragId !== list.id ? "border-t-2 border-blue-500" : ""
            } ${
              selectedListId === list.id ? "bg-blue-100 font-medium" : "hover:bg-gray-100"
            } ${dragId === list.id ? "opacity-50" : ""} cursor-grab active:cursor-grabbing`}
          >
            <button
              className="flex min-w-0 flex-1 items-center gap-2 text-left"
              onClick={() => selectList(list.id)}
            >
              <span
                className="h-3 w-3 shrink-0 rounded-full"
                style={{ backgroundColor: list.color }}
              />
              {editingId === list.id ? (
                <input
                  autoFocus
                  className="min-w-0 flex-1 rounded border px-1 py-0.5 text-sm"
                  value={editingName}
                  onChange={(e) => setEditingName(e.target.value)}
                  onBlur={() => void submitRename(list.id)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") void submitRename(list.id);
                    if (e.key === "Escape") setEditingId(null);
                  }}
                  onClick={(e) => e.stopPropagation()}
                />
              ) : (
                <span className="truncate text-sm">{list.name}</span>
              )}
              <span className="ml-auto shrink-0 rounded-full bg-gray-200 px-1.5 text-xs text-gray-600">
                {countFor(list.id)}
              </span>
            </button>
            {list.id !== "inbox" && (
              <span className="hidden shrink-0 gap-1 group-hover:flex">
                <button
                  className="rounded px-1 text-xs text-gray-500 hover:bg-gray-200"
                  title="重命名"
                  onClick={() => {
                    setEditingId(list.id);
                    setEditingName(list.name);
                  }}
                >
                  ✎
                </button>
                <button
                  className="rounded px-1 text-xs text-gray-500 hover:bg-red-100 hover:text-red-600"
                  title="删除列表"
                  onClick={() => {
                    if (window.confirm(`删除列表「${list.name}」？其下任务会一并删除。`)) {
                      void deleteList(list.id).catch((e: unknown) =>
                        setError(e instanceof Error ? e.message : String(e)),
                      );
                    }
                  }}
                >
                  ×
                </button>
              </span>
            )}
          </div>
        ))}
      </nav>
      <div className="border-t p-3">
        <div className="flex gap-2">
          <input
            className="min-w-0 flex-1 rounded-lg border px-2 py-1.5 text-sm"
            placeholder="新建列表…"
            value={newName}
            onChange={(e) => setNewName(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") void submitNew();
            }}
          />
          <button
            className="shrink-0 rounded-lg bg-blue-600 px-3 text-sm text-white hover:bg-blue-700"
            onClick={() => void submitNew()}
          >
            添加
          </button>
        </div>
      </div>
    </div>
  );
}
