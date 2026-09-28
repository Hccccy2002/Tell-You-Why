import { useMemo, useState } from "react";
import type { KnowledgeCard, OnboardingInput, TopicPreference } from "../types";

interface Props {
  topics: TopicPreference[];
  previewCard: KnowledgeCard;
  busy: boolean;
  onComplete: (input: OnboardingInput) => Promise<void>;
}

const riskyWords = [
  "医疗诊断",
  "法律建议",
  "投资建议",
  "实时政治",
  "博彩",
  "成人内容",
];

export function Onboarding({ topics, previewCard, busy, onComplete }: Props) {
  const [step, setStep] = useState(0);
  const [selected, setSelected] = useState<string[]>([]);
  const [customText, setCustomText] = useState("");
  const [customInterests, setCustomInterests] = useState<string[]>([]);
  const [error, setError] = useState<string | null>(null);

  const selectedCount = selected.length + customInterests.length;
  const canContinue = selectedCount >= 3;
  const progressLabel = useMemo(() => `第 ${step + 1} 步，共 2 步`, [step]);

  function toggleTopic(id: string) {
    setSelected((value) =>
      value.includes(id) ? value.filter((item) => item !== id) : [...value, id],
    );
    setError(null);
  }

  function addCustom() {
    const value = customText.trim();
    if (!value) return;
    if ([...value].length > 30) {
      setError("单个自定义兴趣不能超过 30 个中文字符。");
      return;
    }
    if (riskyWords.some((word) => value.includes(word))) {
      setError("这个主题不在 MVP 的内容范围内，请换一个更通用、低风险的兴趣。");
      return;
    }
    if (customInterests.includes(value)) {
      setError("这个兴趣已经添加过了。");
      return;
    }
    setCustomInterests((values) => [...values, value]);
    setCustomText("");
    setError(null);
  }

  return (
    <main className="onboarding">
      <div className="onboarding-progress" aria-label={progressLabel}>
        {[0, 1].map((index) => (
          <span key={index} className={index <= step ? "active" : ""} />
        ))}
      </div>

      {step === 0 ? (
        <section className="welcome-step">
          <span className="eyebrow">欢迎来到 Tell You Why</span>
          <h1>每天，弄懂一个为什么。</h1>
          <p>先想一想，再用一分钟看懂答案。</p>
          <aside className="preview-question">
            <span>今天的问题</span>
            <strong>{previewCard.question}</strong>
          </aside>
          <p className="onboarding-privacy">无需注册，记录默认保存在本机。</p>
          <button className="primary-button wide" onClick={() => setStep(1)}>
            选择我的兴趣
          </button>
        </section>
      ) : null}

      {step === 1 ? (
        <section className="interest-step">
          <span className="eyebrow">选择兴趣</span>
          <h1>你想多看到哪些内容？</h1>
          <p className="supporting">至少选择 3 个，稍后可以随时修改。</p>
          <div className="topic-grid">
            {topics
              .filter((topic) => !topic.custom)
              .map((topic) => (
                <button
                  key={topic.id}
                  className={
                    selected.includes(topic.id)
                      ? "topic-option selected"
                      : "topic-option"
                  }
                  aria-pressed={selected.includes(topic.id)}
                  onClick={() => toggleTopic(topic.id)}
                >
                  <span className="selection-dot" aria-hidden="true" />
                  {topic.label}
                </button>
              ))}
          </div>
          <div className="custom-interest-row">
            <label htmlFor="custom-interest">自定义兴趣（可选）</label>
            <div>
              <input
                id="custom-interest"
                value={customText}
                maxLength={30}
                placeholder="例如：建筑设计"
                onChange={(event) => setCustomText(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === "Enter") {
                    event.preventDefault();
                    addCustom();
                  }
                }}
              />
              <button className="secondary-button" onClick={addCustom}>
                添加
              </button>
            </div>
          </div>
          {customInterests.length ? (
            <div className="tag-list" aria-label="已添加的自定义兴趣">
              {customInterests.map((item) => (
                <button
                  key={item}
                  onClick={() =>
                    setCustomInterests((items) =>
                      items.filter((value) => value !== item),
                    )
                  }
                >
                  {item} <span aria-hidden="true">×</span>
                </button>
              ))}
            </div>
          ) : null}
          {error ? (
            <p className="field-error" role="alert">
              {error}
            </p>
          ) : null}
          <div className="sticky-actions">
            <span>{selectedCount}/3 已选择</span>
            <button
              className="primary-button"
              disabled={!canContinue || busy}
              onClick={() =>
                void onComplete({
                  selectedTopicIds: selected,
                  customInterests,
                  reminderPreset: "manual",
                })
              }
            >
              {busy ? "正在保存…" : "开始探索"}
            </button>
          </div>
        </section>
      ) : null}
    </main>
  );
}
