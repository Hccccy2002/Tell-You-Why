import { useState } from "react";
import {
  deleteProviderKey,
  friendlyError,
  saveProviderProfile,
  testProviderConnection,
} from "../lib/api";
import type { ProviderSpec } from "../types";

interface Props {
  initialProviders: ProviderSpec[];
  onProvidersChanged: (providers: ProviderSpec[]) => void;
}

export function ModelSettingsScreen({
  initialProviders,
  onProvidersChanged,
}: Props) {
  const [providers, setProviders] = useState(() =>
    structuredClone(initialProviders),
  );
  const [activeId, setActiveId] = useState<ProviderSpec["id"]>(
    initialProviders[0]?.id ?? "deepseek",
  );
  const [apiKey, setApiKey] = useState("");
  const [busy, setBusy] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);

  const active =
    providers.find((provider) => provider.id === activeId) ?? providers[0];

  function updateActive(patch: Partial<ProviderSpec>) {
    setProviders((items) =>
      items.map((item) =>
        item.id === activeId ? { ...item, ...patch } : item,
      ),
    );
    setMessage(null);
  }

  async function saveProfile() {
    if (!active) return;
    if (!active.keyConfigured && !apiKey.trim()) {
      setMessage(
        "请输入完整 API Key。Key 只会交给 Rust 核心并保存到 Windows 凭据库。",
      );
      return;
    }
    const submittedKey = apiKey.trim() || null;
    setApiKey("");
    setBusy("save");
    try {
      const saved = await saveProviderProfile({
        providerId: active.id,
        region: active.selectedRegion,
        model: active.selectedModel,
        apiKey: submittedKey,
      });
      const next = providers.map((item) =>
        item.id === saved.id ? saved : item,
      );
      setProviders(next);
      onProvidersChanged(next);
      setMessage("配置已保存。原始 Key 已从输入框清除。");
    } catch (error) {
      setMessage(friendlyError(error));
    } finally {
      setBusy(null);
    }
  }

  async function testConnection() {
    if (!active) return;
    setBusy("test");
    try {
      const result = await testProviderConnection(
        active.id,
        active.selectedRegion,
      );
      updateActive({ connectionVerified: true });
      const next = providers.map((item) =>
        item.id === active.id ? { ...item, connectionVerified: true } : item,
      );
      setProviders(next);
      onProvidersChanged(next);
      setMessage(result);
    } catch (error) {
      updateActive({ connectionVerified: false });
      setMessage(friendlyError(error));
    } finally {
      setBusy(null);
    }
  }

  async function removeKey() {
    if (!active || !window.confirm(`删除 ${active.label} 的系统凭据？`)) return;
    setBusy("delete");
    try {
      await deleteProviderKey(active.id, active.selectedRegion);
      const next = providers.map((item) =>
        item.id === active.id
          ? {
              ...item,
              keyConfigured: false,
              keyLast4: null,
              connectionVerified: false,
            }
          : item,
      );
      setProviders(next);
      onProvidersChanged(next);
      setApiKey("");
      setMessage("系统凭据已删除。");
    } catch (error) {
      setMessage(friendlyError(error));
    } finally {
      setBusy(null);
    }
  }

  if (!active) return <main className="page-view">供应商注册表暂不可用。</main>;

  return (
    <main className="page-view model-settings">
      <div className="page-heading">
        <span className="eyebrow">可选功能 · BYOK</span>
        <h1>模型设置</h1>
        <p>
          不配置模型也能完整使用。真实 Key
          只能在这里输入，请不要发送到聊天或日志。
        </p>
      </div>
      <div className="provider-tabs" role="tablist" aria-label="模型供应商">
        {providers.map((provider) => (
          <button
            key={provider.id}
            role="tab"
            aria-selected={provider.id === active.id}
            className={provider.id === active.id ? "active" : ""}
            onClick={() => {
              setActiveId(provider.id);
              setApiKey("");
              setMessage(null);
            }}
          >
            {provider.label}
            {provider.keyConfigured ? (
              <span className="configured-dot" aria-label="已配置" />
            ) : null}
          </button>
        ))}
      </div>
      <section className="settings-card">
        <label className="field-label">
          服务通道
          <select
            value={active.selectedRegion}
            onChange={(event) =>
              updateActive({
                selectedRegion: event.target.value,
                connectionVerified: false,
              })
            }
          >
            {active.regions.map((region) => (
              <option key={region.id} value={region.id}>
                {region.label}
              </option>
            ))}
          </select>
        </label>
        <label className="field-label">
          模型
          <select
            value={active.selectedModel}
            onChange={(event) =>
              updateActive({
                selectedModel: event.target.value,
                connectionVerified: false,
              })
            }
          >
            {active.models.map((model) => (
              <option key={model.id} value={model.id}>
                {model.label}
                {model.recommended ? "（推荐）" : ""}
              </option>
            ))}
          </select>
        </label>
        <label className="field-label">
          API Key
          <input
            type="password"
            autoComplete="off"
            spellCheck={false}
            value={apiKey}
            placeholder={
              active.keyConfigured
                ? `已配置 · •••• ${active.keyLast4 ?? ""}`
                : "输入完整 Key"
            }
            onChange={(event) => setApiKey(event.target.value)}
          />
        </label>
        <p className="security-note">
          <span aria-hidden="true">▣</span>
          Key 由 Rust 保存到 Windows Credential Manager；SQLite
          和前端都不保存原文。
        </p>
        <button
          className="secondary-button wide"
          disabled={busy != null}
          onClick={() => void saveProfile()}
        >
          {busy === "save" ? "正在安全保存…" : "保存配置"}
        </button>
        {active.keyConfigured ? (
          <div className="connection-row">
            <span
              className={
                active.connectionVerified ? "status success" : "status"
              }
            >
              {active.connectionVerified ? "连接已验证" : "尚未验证连接"}
            </span>
            <button
              className="text-button"
              disabled={busy != null}
              onClick={() => void testConnection()}
            >
              {busy === "test" ? "正在测试…" : "测试连接"}
            </button>
          </div>
        ) : null}
      </section>
      <p className="model-call-note">
        仅在你点击知识小窗中的生成按钮时调用模型，不会后台自动补充。
      </p>
      {message ? (
        <p className="form-message" role="status">
          {message}
        </p>
      ) : null}
      {active.keyConfigured ? (
        <button
          className="danger-text-button"
          disabled={busy != null}
          onClick={() => void removeKey()}
        >
          {busy === "delete" ? "正在删除…" : `删除 ${active.label} API Key`}
        </button>
      ) : null}
      <p className="ai-disclosure">
        模型生成的卡片统一标记为“AI
        生成，未经外部核验”，模型自行给出的链接不会显示为已核验来源。
      </p>
    </main>
  );
}
