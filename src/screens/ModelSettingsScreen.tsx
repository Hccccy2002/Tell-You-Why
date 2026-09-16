import { useState } from "react";
import {
  CREDENTIAL_REPLACEMENT_REQUIRED,
  deleteProviderKey,
  friendlyError,
  saveProviderProfile,
  testProviderConnection,
} from "../lib/api";
import { ConfirmationDialog } from "../components/ConfirmationDialog";
import { PdfModelOverview } from "../components/PdfModelOverview";
import type { ProviderSpec } from "../types";

interface ProviderKeyDeleteConfirmation {
  providerId: ProviderSpec["id"];
  region: string;
  regionLabel: string;
  label: string;
  keyLast4: string | null;
}

interface ProviderKeyReplaceConfirmation {
  providerId: ProviderSpec["id"];
  region: string;
  model: string;
  apiKey: string;
  label: string;
  regionLabel: string;
  keyLast4: string | null;
}

interface Props {
  initialProviders: ProviderSpec[];
  onProvidersChanged: (providers: ProviderSpec[]) => void;
}

export function ModelSettingsScreen({
  initialProviders,
  onProvidersChanged,
}: Props) {
  const [section, setSection] = useState<"online" | "pdf">("online");
  const [providers, setProviders] = useState(() =>
    structuredClone(initialProviders),
  );
  const [activeId, setActiveId] = useState<ProviderSpec["id"]>(
    initialProviders[0]?.id ?? "deepseek",
  );
  const [apiKey, setApiKey] = useState("");
  const [busy, setBusy] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [deleteConfirmation, setDeleteConfirmation] =
    useState<ProviderKeyDeleteConfirmation | null>(null);
  const [replaceConfirmation, setReplaceConfirmation] =
    useState<ProviderKeyReplaceConfirmation | null>(null);

  const active =
    providers.find((provider) => provider.id === activeId) ?? providers[0];
  const controlsLocked =
    busy != null || deleteConfirmation != null || replaceConfirmation != null;

  function updateActive(patch: Partial<ProviderSpec>) {
    setProviders((items) =>
      items.map((item) =>
        item.id === activeId ? { ...item, ...patch } : item,
      ),
    );
    setMessage(null);
  }

  function replacementTarget(
    provider: ProviderSpec,
    submittedKey: string,
  ): ProviderKeyReplaceConfirmation {
    return {
      providerId: provider.id,
      region: provider.selectedRegion,
      model: provider.selectedModel,
      apiKey: submittedKey,
      label: provider.label,
      regionLabel:
        provider.regions.find((region) => region.id === provider.selectedRegion)
          ?.label ?? provider.selectedRegion,
      keyLast4: provider.keyLast4,
    };
  }

  function requestSaveProfile() {
    if (!active) return;
    const submittedKey = apiKey.trim();
    if (!active.keyConfigured && !submittedKey) {
      setMessage(
        "请输入完整 API Key。Key 只会交给 Rust 核心并保存到 Windows 凭据库。",
      );
      return;
    }
    const target = replacementTarget(active, submittedKey);
    if (active.keyConfigured && submittedKey) {
      setMessage(null);
      setReplaceConfirmation(target);
      return;
    }

    void submitProfile(target, false);
  }

  async function submitProfile(
    target: ProviderKeyReplaceConfirmation,
    replacementConfirmed: boolean,
  ) {
    setBusy("save");
    try {
      const saved = await saveProviderProfile({
        providerId: target.providerId,
        region: target.region,
        model: target.model,
        apiKey: target.apiKey || null,
        replaceExistingKey: replacementConfirmed,
      });
      const next = providers.map((item) =>
        item.id === saved.id ? saved : item,
      );
      setProviders(next);
      onProvidersChanged(next);
      setReplaceConfirmation(null);
      setApiKey("");
      setMessage("配置已保存。原始 Key 已从输入框清除。");
    } catch (error) {
      const text = friendlyError(error);
      if (
        !replacementConfirmed &&
        target.apiKey &&
        text === CREDENTIAL_REPLACEMENT_REQUIRED
      ) {
        setReplaceConfirmation(target);
        setMessage(null);
      } else {
        setMessage(
          replacementConfirmed
            ? `${text} API Key 未保存，可直接重试或取消后修改。`
            : text,
        );
      }
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

  async function removeKey(target: ProviderKeyDeleteConfirmation) {
    setBusy("delete");
    try {
      await deleteProviderKey(target.providerId, target.region);
      const next = providers.map((item) =>
        item.id === target.providerId
          ? {
              ...item,
              selectedRegion: item.regions[0]?.id ?? item.selectedRegion,
              selectedModel:
                item.models.find((model) => model.recommended)?.id ??
                item.models[0]?.id ??
                item.selectedModel,
              keyConfigured: false,
              keyLast4: null,
              connectionVerified: false,
            }
          : item,
      );
      setProviders(next);
      onProvidersChanged(next);
      setApiKey("");
      setDeleteConfirmation(null);
      setMessage("系统凭据已删除。");
    } catch (error) {
      setMessage(friendlyError(error));
    } finally {
      setBusy(null);
    }
  }

  return (
    <main className="page-view model-settings">
      <div className="page-heading">
        <span className="eyebrow">在线生成 · 本地识别与检索</span>
        <h1>模型设置</h1>
        <p>配置生成与追问使用的在线模型，查看 PDF 知识库使用的本地模型。</p>
      </div>
      <div
        className="segmented-control model-section-switcher"
        role="group"
        aria-label="模型设置分类"
      >
        <button
          type="button"
          className={section === "online" ? "active" : ""}
          aria-pressed={section === "online"}
          aria-controls="online-model-settings"
          disabled={controlsLocked}
          onClick={() => setSection("online")}
        >
          在线生成模型
        </button>
        <button
          type="button"
          className={section === "pdf" ? "active" : ""}
          aria-pressed={section === "pdf"}
          aria-controls="pdf-model-settings"
          disabled={controlsLocked}
          onClick={() => setSection("pdf")}
        >
          PDF 知识库模型
        </button>
      </div>
      <div id="pdf-model-settings" hidden={section !== "pdf"}>
        <PdfModelOverview />
      </div>
      <div id="online-model-settings" hidden={section !== "online"}>
        {active ? (
          <>
            <div
              className="provider-tabs"
              role="tablist"
              aria-label="模型供应商"
            >
              {providers.map((provider) => (
                <button
                  key={provider.id}
                  role="tab"
                  disabled={controlsLocked}
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
                  disabled={controlsLocked}
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
                  disabled={controlsLocked}
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
                  disabled={controlsLocked}
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
                disabled={controlsLocked}
                onClick={requestSaveProfile}
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
                    disabled={controlsLocked}
                    onClick={() => void testConnection()}
                  >
                    {busy === "test" ? "正在测试…" : "测试连接"}
                  </button>
                </div>
              ) : null}
            </section>
            <p className="model-call-note">
              在线模型用于知识卡生成、追问，以及 PDF
              中的生成、解释和复习。仅在你主动使用这些功能时调用，不会后台自动补充。
            </p>
            {message && !deleteConfirmation && !replaceConfirmation ? (
              <p className="form-message" role="status">
                {message}
              </p>
            ) : null}
            {active.keyConfigured ? (
              <button
                className="danger-text-button"
                disabled={controlsLocked}
                onClick={() => {
                  setMessage(null);
                  setDeleteConfirmation({
                    providerId: active.id,
                    region: active.selectedRegion,
                    regionLabel:
                      active.regions.find(
                        (region) => region.id === active.selectedRegion,
                      )?.label ?? active.selectedRegion,
                    label: active.label,
                    keyLast4: active.keyLast4,
                  });
                }}
              >
                {busy === "delete"
                  ? "正在删除…"
                  : `删除 ${active.label} API Key`}
              </button>
            ) : null}
            <p className="ai-disclosure">
              模型生成的卡片统一标记为“AI
              生成，未经外部核验”，模型自行给出的链接不会显示为已核验来源。
            </p>
          </>
        ) : (
          <p className="form-message" role="status">
            供应商注册表暂不可用。
          </p>
        )}
      </div>
      {deleteConfirmation ? (
        <ConfirmationDialog
          id="provider-key-delete-confirmation"
          eyebrow="凭据删除确认"
          title={`确认删除 ${deleteConfirmation.label} API Key 吗？`}
          confirmLabel="删除 API Key"
          busyLabel="正在删除 API Key…"
          busy={busy === "delete"}
          onCancel={() => setDeleteConfirmation(null)}
          onConfirm={() => void removeKey(deleteConfirmation)}
        >
          <p>
            将从 Windows Credential Manager 删除 {deleteConfirmation.label} ·
            {deleteConfirmation.regionLabel}
            {deleteConfirmation.keyLast4
              ? ` · 尾号 ${deleteConfirmation.keyLast4}`
              : ""}
            的系统凭据，并移除该服务通道已保存的模型选择和连接验证状态。之后如需调用该模型，必须重新配置。
          </p>
          {message ? (
            <p className="inline-error" role="alert">
              {message}
            </p>
          ) : null}
        </ConfirmationDialog>
      ) : null}
      {replaceConfirmation ? (
        <ConfirmationDialog
          id="provider-key-replace-confirmation"
          eyebrow="凭据覆盖确认"
          title={`确认覆盖 ${replaceConfirmation.label} API Key 吗？`}
          confirmLabel="确认覆盖 API Key"
          busyLabel="正在安全保存…"
          busy={busy === "save"}
          onCancel={() => {
            setReplaceConfirmation(null);
            setMessage(null);
          }}
          onConfirm={() => void submitProfile(replaceConfirmation, true)}
        >
          <p>
            将覆盖 {replaceConfirmation.label} ·{" "}
            {replaceConfirmation.regionLabel}
            {replaceConfirmation.keyLast4
              ? ` · 尾号 ${replaceConfirmation.keyLast4}`
              : ""}
            的现有系统凭据，旧 API Key 将无法恢复。
          </p>
          {message ? (
            <p className="inline-error" role="alert">
              {message}
            </p>
          ) : null}
        </ConfirmationDialog>
      ) : null}
    </main>
  );
}
