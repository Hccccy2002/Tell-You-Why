import { useEffect, useState } from "react";
import { CREDENTIAL_REPLACEMENT_REQUIRED, friendlyError } from "../lib/api";
import {
  deleteSearchKey,
  getSearchSettings,
  saveSearchKey,
  testSearchConnection,
  type SearchSettings,
  type SearchOptions,
  defaultSearchOptions,
  saveSearchOptions,
} from "../lib/search";
import { ConfirmationDialog } from "./ConfirmationDialog";

export function SearchSettingsPanel({
  onBusyChange,
  onSettingsChange,
}: {
  onBusyChange: (busy: boolean) => void;
  onSettingsChange?: (settings: SearchSettings) => void;
}) {
  const [settings, setSettings] = useState<SearchSettings | null>(null);
  const [apiKey, setApiKey] = useState("");
  const [options, setOptions] = useState<SearchOptions>(defaultSearchOptions);
  const [busy, setBusy] = useState<"save" | "test" | "delete" | "load" | null>(
    "load",
  );
  const [confirmation, setConfirmation] = useState<"replace" | "delete" | null>(
    null,
  );
  const [message, setMessage] = useState<string | null>(null);
  const locked = busy !== null || confirmation !== null;

  useEffect(() => {
    let active = true;
    void getSearchSettings()
      .then((value) => {
        if (active) {
          setSettings(value);
          setOptions(value.options ?? defaultSearchOptions);
        }
      })
      .catch((error: unknown) => {
        if (active) setMessage(friendlyError(error));
      })
      .finally(() => {
        if (active) setBusy(null);
      });
    return () => {
      active = false;
    };
  }, []);

  useEffect(() => {
    onBusyChange(locked);
    return () => onBusyChange(false);
  }, [locked, onBusyChange]);

  useEffect(() => {
    if (settings) onSettingsChange?.(settings);
  }, [settings, onSettingsChange]);

  async function save(confirmed = false) {
    if (!apiKey.trim()) {
      setMessage("请输入完整智谱 API Key。");
      return;
    }
    if (settings?.keyConfigured && !confirmed) {
      setMessage(null);
      setConfirmation("replace");
      return;
    }
    setBusy("save");
    setMessage(null);
    try {
      setSettings(await saveSearchKey(apiKey.trim(), confirmed));
      setApiKey("");
      setConfirmation(null);
      setMessage("智谱 API Key 已保存，输入框已清空。可以测试连接。");
    } catch (error) {
      const text = friendlyError(error);
      if (text === CREDENTIAL_REPLACEMENT_REQUIRED) setConfirmation("replace");
      else setMessage(text);
    } finally {
      setBusy(null);
    }
  }

  async function test() {
    setBusy("test");
    setMessage(null);
    try {
      setSettings(await testSearchConnection());
      setMessage("智谱连接成功，已返回可用搜索结果。");
    } catch (error) {
      setSettings((value) =>
        value ? { ...value, connectionVerified: false } : value,
      );
      setMessage(friendlyError(error));
    } finally {
      setBusy(null);
    }
  }

  async function saveOptions() {
    setBusy("save");
    setMessage(null);
    try {
      setSettings(await saveSearchOptions(options));
      setMessage("搜索设置已保存。");
    } catch (error) {
      setMessage(friendlyError(error));
    } finally {
      setBusy(null);
    }
  }

  async function remove() {
    setBusy("delete");
    setMessage(null);
    try {
      await deleteSearchKey();
      setOptions((current) => ({ ...current, mode: "off" }));
      setSettings({
        keyConfigured: false,
        keyLast4: null,
        connectionVerified: false,
      });
      setApiKey("");
      setConfirmation(null);
      setMessage("智谱系统凭据已删除。");
    } catch (error) {
      setMessage(friendlyError(error));
    } finally {
      setBusy(null);
    }
  }

  return (
    <>
      <section className="settings-card" aria-label="智谱搜索配置">
        <h2>智谱搜索</h2>
        {busy === "load" ? <p role="status">正在读取配置…</p> : null}
        <label className="field-label">
          智谱 API Key
          <input
            type="password"
            autoComplete="off"
            spellCheck={false}
            value={apiKey}
            disabled={locked || !settings}
            placeholder={
              settings?.keyConfigured
                ? `已配置 · •••• ${settings.keyLast4 ?? ""}`
                : "输入智谱开放平台 API Key"
            }
            onChange={(event) => setApiKey(event.target.value)}
          />
        </label>
        <p className="security-note">
          Key 保存到 Windows 凭据库，输入框在保存后清空。
        </p>
        <button
          className="secondary-button wide"
          disabled={locked || !settings}
          onClick={() => void save()}
        >
          {busy === "save" ? "正在安全保存…" : "保存智谱配置"}
        </button>
        {settings?.keyConfigured ? (
          <>
            <div className="connection-row">
              <span
                className={
                  settings.connectionVerified ? "status success" : "status"
                }
              >
                {settings.connectionVerified ? "连接已验证" : "尚未验证连接"}
              </span>
              <button
                className="text-button"
                disabled={
                  locked ||
                  Boolean(apiKey.trim()) ||
                  options.engine !== (settings.options?.engine ?? "search_pro")
                }
                onClick={() => void test()}
              >
                {busy === "test" ? "正在测试…" : "测试智谱连接"}
              </button>
            </div>
            {apiKey.trim() ? (
              <p className="model-call-note">请先保存新 Key，再测试连接。</p>
            ) : null}
            {options.engine !== (settings.options?.engine ?? "search_pro") ? (
              <p className="model-call-note">
                请先保存搜索引擎设置，再测试连接。
              </p>
            ) : null}
          </>
        ) : null}
      </section>
      <section className="settings-card" aria-label="联网搜索设置">
        <label className="field-label">
          搜索模式
          <select
            value={options.mode}
            disabled={locked || !settings}
            onChange={(event) =>
              setOptions({
                ...options,
                mode: event.target.value as SearchOptions["mode"],
              })
            }
          >
            <option value="off">关闭</option>
            <option value="auto">自动判断</option>
            <option value="always">每次联网</option>
          </select>
        </label>
        <label className="field-label">
          搜索引擎
          <select
            value={options.engine}
            disabled={locked || !settings}
            onChange={(event) =>
              setOptions({
                ...options,
                engine: event.target.value as SearchOptions["engine"],
              })
            }
          >
            <option value="search_pro">Search-Pro · 专业搜索</option>
            <option value="search_std">Search-Std · 基础搜索</option>
          </select>
        </label>
        <label className="field-label">
          每日搜索次数上限
          <input
            type="number"
            min={1}
            max={500}
            value={options.dailyAttemptLimit}
            disabled={locked || !settings}
            onChange={(event) =>
              setOptions({
                ...options,
                dailyAttemptLimit: Number(event.target.value),
              })
            }
          />
        </label>
        <p className="model-call-note">
          今日已尝试 {settings?.attemptsToday ?? 0}{" "}
          次，含连接测试和补搜。开启后，必要查询发送至智谱，选中的资料发送至当前回答模型。
        </p>
        <button
          className="secondary-button wide"
          disabled={
            locked ||
            !settings ||
            !Number.isInteger(options.dailyAttemptLimit) ||
            options.dailyAttemptLimit < 1 ||
            options.dailyAttemptLimit > 500
          }
          onClick={() => void saveOptions()}
        >
          保存搜索设置
        </button>
      </section>
      {message && !confirmation ? (
        <p className="form-message" role="status">
          {message}
        </p>
      ) : null}
      {settings?.keyConfigured ? (
        <button
          className="danger-text-button"
          disabled={locked}
          onClick={() => {
            setMessage(null);
            setConfirmation("delete");
          }}
        >
          删除智谱 API Key
        </button>
      ) : null}
      {confirmation ? (
        <ConfirmationDialog
          id="search-key-confirmation"
          title={
            confirmation === "delete"
              ? "确认删除智谱 API Key 吗？"
              : "确认覆盖智谱 API Key 吗？"
          }
          confirmLabel={
            confirmation === "delete" ? "删除 API Key" : "确认覆盖 API Key"
          }
          busyLabel="正在处理…"
          busy={busy !== null}
          onCancel={() => {
            setConfirmation(null);
            setMessage(null);
          }}
          onConfirm={() => {
            if (confirmation === "delete") void remove();
            else void save(true);
          }}
        >
          <p>
            {confirmation === "delete"
              ? "将删除智谱搜索的系统凭据，需要重新配置后才能使用。"
              : "将替换已保存的智谱搜索 Key，旧 Key 将无法从 APP 恢复。"}
          </p>
          {message ? (
            <p className="inline-error" role="alert">
              {message}
            </p>
          ) : null}
        </ConfirmationDialog>
      ) : null}
    </>
  );
}
