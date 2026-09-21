import { useMemo, useState } from "react";
import type { ProviderSpec, TopicPreference } from "../types";

const CUSTOM_TOPIC = "__custom__";

interface Props {
  topics: TopicPreference[];
  providers: ProviderSpec[];
  generationProviderId: "deepseek" | "kimi";
  availableCardCount: number;
  maxGenerationCount: number;
  busy: boolean;
  pendingGeneration: {
    topicLabel: string;
    completed: number;
    total: number;
    remaining: number;
  } | null;
  onBrowse: () => Promise<void>;
  onGenerate: (
    topicId: string | null,
    topicLabel: string,
    count: number,
  ) => Promise<void>;
  onGenerateRandom: (count: number) => Promise<void>;
  onContinueGeneration: () => Promise<void>;
  onOpenModelSettings: () => void;
  onGenerationProviderChange: (
    providerId: "deepseek" | "kimi",
  ) => Promise<void>;
}

export function KnowledgeHome({
  topics,
  providers,
  generationProviderId,
  availableCardCount,
  maxGenerationCount,
  busy,
  pendingGeneration,
  onBrowse,
  onGenerate,
  onGenerateRandom,
  onContinueGeneration,
  onOpenModelSettings,
  onGenerationProviderChange,
}: Props) {
  const enabledTopics = useMemo(
    () =>
      topics
        .filter((topic) => topic.enabled)
        .sort(
          (left, right) =>
            Number(right.selected) - Number(left.selected) ||
            left.rank - right.rank,
        ),
    [topics],
  );
  const [selection, setSelection] = useState(
    enabledTopics[0]?.id ?? CUSTOM_TOPIC,
  );
  const [customTopic, setCustomTopic] = useState("");
  const [generationCountInput, setGenerationCountInput] = useState("1");
  const generationLimit = Math.max(1, maxGenerationCount);
  const parsedGenerationCount = Number.parseInt(generationCountInput, 10);
  const generationCount =
    Number.isFinite(parsedGenerationCount) && parsedGenerationCount > 0
      ? Math.min(generationLimit, parsedGenerationCount)
      : 0;
  const selectedTopic = enabledTopics.find((topic) => topic.id === selection);
  const topicLabel =
    selection === CUSTOM_TOPIC
      ? customTopic.trim()
      : (selectedTopic?.label ?? "");
  const interestedTopics = enabledTopics.filter((topic) => topic.selected);
  const otherTopics = enabledTopics.filter((topic) => !topic.selected);
  const selectedProvider = providers.find(
    (provider) => provider.id === generationProviderId,
  );

  return (
    <main className="feed-empty" aria-labelledby="feed-empty-title">
      <div className="feed-empty-card">
        <span className="eyebrow">知识小窗</span>
        <h1 id="feed-empty-title">想探索哪个领域？</h1>
        <p>选择一个领域，生成新的知识卡。</p>
        <div className="generation-provider-field">
          <div className="generation-provider-heading">
            <label htmlFor="generation-provider">生成模型</label>
            {selectedProvider && !selectedProvider.connectionVerified ? (
              <button
                className="generation-provider-setup"
                type="button"
                disabled={busy}
                onClick={onOpenModelSettings}
              >
                {selectedProvider.keyConfigured ? "去测试" : "去配置"}
                <span aria-hidden="true"> →</span>
              </button>
            ) : null}
          </div>
          <select
            id="generation-provider"
            aria-label="选择生成模型"
            value={generationProviderId}
            disabled={busy}
            onChange={(event) =>
              void onGenerationProviderChange(
                event.currentTarget.value as "deepseek" | "kimi",
              )
            }
          >
            {providers.map((provider) => {
              const model =
                provider.models.find(
                  (item) => item.id === provider.selectedModel,
                )?.label ?? provider.selectedModel;
              const status = provider.connectionVerified
                ? "已就绪"
                : provider.keyConfigured
                  ? "待连接测试"
                  : "未配置";
              return (
                <option key={provider.id} value={provider.id}>
                  {provider.label} · {model} · {status}
                </option>
              );
            })}
          </select>
          <small>首选模型不可用时，将自动尝试另一已就绪模型。</small>
        </div>
        {pendingGeneration ? (
          <section className="pending-generation" aria-label="未完成的生成任务">
            <span>
              已生成 {pendingGeneration.completed}/{pendingGeneration.total} 条
            </span>
            <strong>
              还有 {pendingGeneration.remaining} 条
              {pendingGeneration.topicLabel}知识点未完成
            </strong>
            <button disabled={busy} onClick={() => void onContinueGeneration()}>
              {busy
                ? "正在继续生成…"
                : `继续生成剩余 ${pendingGeneration.remaining} 条`}
            </button>
          </section>
        ) : null}
        <div className="choice-divider">
          <span>生成新知识</span>
        </div>
        <label className="generation-count-field">
          <span>本次生成</span>
          <input
            type="number"
            aria-label="本次生成知识卡数量"
            min={1}
            max={generationLimit}
            value={generationCountInput}
            onChange={(event) =>
              setGenerationCountInput(event.currentTarget.value)
            }
            onBlur={() => setGenerationCountInput(String(generationCount || 1))}
            onWheel={(event) => event.currentTarget.blur()}
          />
          <span>条知识卡</span>
        </label>
        <form
          onSubmit={(event) => {
            event.preventDefault();
            if (!busy && topicLabel && generationCount > 0) {
              void onGenerate(
                selectedTopic?.id ?? null,
                topicLabel,
                generationCount,
              );
            }
          }}
        >
          <label className="feed-topic-field">
            <span>知识领域</span>
            <select
              aria-label="选择知识领域"
              value={selection}
              onChange={(event) => setSelection(event.target.value)}
            >
              {interestedTopics.length > 0 ? (
                <optgroup label="我的兴趣">
                  {interestedTopics.map((topic) => (
                    <option key={topic.id} value={topic.id}>
                      {topic.label}
                    </option>
                  ))}
                </optgroup>
              ) : null}
              <option value={CUSTOM_TOPIC}>自定义其它领域…</option>
              {otherTopics.length > 0 ? (
                <optgroup label="其它已有领域">
                  {otherTopics.map((topic) => (
                    <option key={topic.id} value={topic.id}>
                      {topic.label}
                    </option>
                  ))}
                </optgroup>
              ) : null}
            </select>
          </label>
          {selection === CUSTOM_TOPIC ? (
            <label className="feed-topic-field">
              <span>自定义领域</span>
              <input
                aria-label="自定义知识领域"
                value={customTopic}
                maxLength={30}
                placeholder="例如：建筑与城市"
                onChange={(event) => setCustomTopic(event.target.value)}
                autoFocus
              />
            </label>
          ) : null}
          <div className="generation-action-row">
            <button
              className="random-empty-button"
              type="button"
              disabled={
                busy || interestedTopics.length === 0 || generationCount === 0
              }
              onClick={() => void onGenerateRandom(generationCount)}
            >
              <span aria-hidden="true">✦</span>
              <span className="generation-action-copy">
                <strong>{busy ? "正在生成…" : "按兴趣权重随机生成"}</strong>
                <small>{generationCount} 条知识卡</small>
              </span>
            </button>
            <button
              className="generate-topic-button"
              type="submit"
              disabled={busy || !topicLabel || generationCount === 0}
            >
              <span className="generation-action-copy">
                <strong>
                  {busy ? "正在生成…" : `生成${topicLabel || "所选领域"}知识点`}
                </strong>
                <small>{generationCount} 条知识卡</small>
              </span>
            </button>
          </div>
        </form>
        {availableCardCount > 0 ? (
          <button
            className="browse-available-button"
            disabled={busy}
            onClick={() => void onBrowse()}
          >
            <span aria-hidden="true">▤</span>
            浏览现有知识点
            <small>{availableCardCount} 张可展示</small>
          </button>
        ) : null}
      </div>
    </main>
  );
}
