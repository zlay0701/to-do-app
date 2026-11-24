import { useEffect, useState } from "react";
import { api, isTauriEnv } from "../lib/api";
import { useTodo } from "../store";
import type { TestResult } from "../lib/types";

/** 设置页：服务端 + 本地通知两个分区（SPEC §5.1、§7）。 */
export function SettingsPage() {
  const settings = useTodo((s) => s.settings);
  const refreshSettings = useTodo((s) => s.refreshSettings);
  const setError = useTodo((s) => s.setError);

  const [serverUrl, setServerUrl] = useState("");
  const [token, setToken] = useState("");
  const [syncIntervalMin, setSyncIntervalMin] = useState(15);
  const [autoSync, setAutoSync] = useState(true);
  const [notifyEnabled, setNotifyEnabled] = useState(true);
  const [beforeMin, setBeforeMin] = useState(0);
  const [testResult, setTestResult] = useState<TestResult | null>(null);
  const [testing, setTesting] = useState(false);
  const [resetResult, setResetResult] = useState<{ ok: boolean; message: string } | null>(null);
  const [resetting, setResetting] = useState(false);
  const resetSyncAction = useTodo((s) => s.resetSync);
  const [savedAt, setSavedAt] = useState<string | null>(null);

  useEffect(() => {
    if (settings) {
      setServerUrl(settings.server.serverUrl);
      setSyncIntervalMin(settings.server.syncIntervalMin);
      setAutoSync(settings.server.autoSync);
      setNotifyEnabled(settings.localNotify.enabled);
      setBeforeMin(settings.localNotify.beforeMin);
    }
  }, [settings]);

  if (!settings) return <div className="p-4 text-sm text-gray-500">加载设置中…</div>;

  const saveServer = async () => {
    try {
      await api.saveSettings("server", {
        serverUrl,
        syncIntervalMin,
        autoSync,
        ...(token ? { token } : {}),
      });
      setToken("");
      setSavedAt(new Date().toLocaleTimeString("zh-CN"));
      await refreshSettings();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const saveNotify = async () => {
    try {
      await api.saveSettings("local_notify", { enabled: notifyEnabled, beforeMin });
      setSavedAt(new Date().toLocaleTimeString("zh-CN"));
      await refreshSettings();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const testConnection = async () => {
    setTesting(true);
    setTestResult(null);
    try {
      // 先保存当前地址与令牌，再测（令牌只写不读，空着表示沿用已保存的）。
      await api.saveSettings("server", {
        serverUrl,
        syncIntervalMin,
        autoSync,
        ...(token ? { token } : {}),
      });
      setTestResult(await api.testServer());
      await refreshSettings();
    } catch (e) {
      setTestResult({ ok: false, status: null, message: e instanceof Error ? e.message : String(e) });
    } finally {
      setTesting(false);
    }
  };

  // v4.4：重置同步（兜底入口）。确认后清空游标与代际标识，下次同步全量重拉；
  // 本地全部记录重新标记 dirty 并重新推送，不会删除本地数据。
  const resetSync = async () => {
    if (
      !window.confirm(
        "确定要重置同步吗？下次同步将全量拉取服务端数据，本地全部记录会重新推送到服务端。本地数据不会被删除。",
      )
    ) {
      return;
    }
    setResetting(true);
    setResetResult(null);
    try {
      const message = await resetSyncAction();
      setResetResult({ ok: true, message });
    } catch (e) {
      setResetResult({ ok: false, message: e instanceof Error ? e.message : String(e) });
    } finally {
      setResetting(false);
    }
  };

  return (
    <div className="mx-auto w-full max-w-2xl px-4 py-4">
      <h1 className="mb-4 text-xl font-bold">设置</h1>
      {savedAt && (
        <p className="mb-3 rounded-lg bg-green-50 px-3 py-2 text-sm text-green-700">
          已保存（{savedAt}）
        </p>
      )}

      <section className="mb-6 rounded-xl border border-gray-200 bg-white p-4 shadow-sm">
        <h2 className="mb-1 font-medium">服务端</h2>
        <p className="mb-3 text-xs text-gray-500">
          多端同步与提醒依赖服务端（第 3 轮上线）。未配置时本应用单机完整可用。
          {!isTauriEnv && " 浏览器模式下令牌不保存原文，完整测试请在桌面端进行。"}
        </p>
        <label className="mb-3 block text-sm">
          服务端地址
          <input
            className="mt-1 w-full rounded-lg border px-3 py-2"
            placeholder="https://todo.example.com"
            value={serverUrl}
            onChange={(e) => setServerUrl(e.target.value)}
          />
        </label>
        <label className="mb-3 block text-sm">
          同步令牌
          <input
            type="password"
            className="mt-1 w-full rounded-lg border px-3 py-2"
            placeholder={settings.hasToken ? "已保存（留空表示不改）" : "粘贴同步令牌"}
            value={token}
            onChange={(e) => setToken(e.target.value)}
            autoComplete="off"
          />
        </label>
        <label className="mb-3 block text-sm">
          自动同步间隔
          <select
            className="mt-1 w-full rounded-lg border px-3 py-2"
            value={syncIntervalMin}
            onChange={(e) => setSyncIntervalMin(Number(e.target.value))}
          >
            <option value={5}>每 5 分钟</option>
            <option value={15}>每 15 分钟</option>
            <option value={30}>每 30 分钟</option>
            <option value={60}>每 60 分钟</option>
          </select>
        </label>
        <label className="mb-4 flex items-center gap-2 text-sm">
          <input
            type="checkbox"
            checked={autoSync}
            onChange={(e) => setAutoSync(e.target.checked)}
          />
          联网时自动同步
        </label>
        <div className="flex flex-wrap gap-2">
          <button
            className="rounded-lg bg-blue-600 px-4 py-2 text-sm text-white hover:bg-blue-700"
            onClick={() => void saveServer()}
          >
            保存服务端设置
          </button>
          <button
            className="rounded-lg border px-4 py-2 text-sm hover:bg-gray-50 disabled:opacity-50"
            disabled={testing}
            onClick={() => void testConnection()}
          >
            {testing ? "测试中…" : "测试连接"}
          </button>
          <button
            className="rounded-lg border border-amber-300 px-4 py-2 text-sm text-amber-700 hover:bg-amber-50 disabled:opacity-50"
            disabled={resetting}
            onClick={() => void resetSync()}
            title="清空同步游标与代际标识，下次同步全量拉取服务端数据（服务端换库、数据对不上时用）"
          >
            {resetting ? "重置中…" : "重置同步"}
          </button>
        </div>
        {resetResult && (
          <p
            className={`mt-3 rounded-lg px-3 py-2 text-sm ${
              resetResult.ok ? "bg-green-50 text-green-700" : "bg-red-50 text-red-700"
            }`}
          >
            {resetResult.ok ? "✓ " : "✕ "}
            {resetResult.message}
          </p>
        )}
        {testResult && (
          <p
            className={`mt-3 rounded-lg px-3 py-2 text-sm ${
              testResult.ok ? "bg-green-50 text-green-700" : "bg-red-50 text-red-700"
            }`}
          >
            {testResult.ok ? "✓ " : "✕ "}
            {testResult.message}
            {testResult.status != null && `（HTTP ${testResult.status}）`}
          </p>
        )}
      </section>

      <section className="rounded-xl border border-gray-200 bg-white p-4 shadow-sm">
        <h2 className="mb-1 font-medium">本地通知</h2>
        <p className="mb-3 text-xs text-gray-500">
          应用在前台时，到期任务会在这里提醒。关掉应用后的提醒（邮件 / Webhook）依赖服务端，第 3 轮上线。
        </p>
        <label className="mb-3 flex items-center gap-2 text-sm">
          <input
            type="checkbox"
            checked={notifyEnabled}
            onChange={(e) => setNotifyEnabled(e.target.checked)}
          />
          启用到期提醒
        </label>
        <label className="mb-4 block text-sm">
          提前提醒
          <select
            className="mt-1 w-full rounded-lg border px-3 py-2"
            value={beforeMin}
            onChange={(e) => setBeforeMin(Number(e.target.value))}
            disabled={!notifyEnabled}
          >
            <option value={0}>到期时</option>
            <option value={5}>提前 5 分钟</option>
            <option value={15}>提前 15 分钟</option>
            <option value={30}>提前 30 分钟</option>
            <option value={60}>提前 1 小时</option>
          </select>
        </label>
        <button
          className="rounded-lg bg-blue-600 px-4 py-2 text-sm text-white hover:bg-blue-700"
          onClick={() => void saveNotify()}
        >
          保存通知设置
        </button>
      </section>
    </div>
  );
}
