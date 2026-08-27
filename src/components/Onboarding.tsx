import { useMemo, useState } from "react";
import type {
  KnowledgeCard,
  OnboardingInput,
  ReminderPreset,
  TopicPreference,
} from "../types";

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
  const [reminderPreset, setReminderPreset] =
    useState<ReminderPreset>("manual");
  const [error, setError] = useState<string | null>(null);

  const selectedCount = selected.length + customInterests.length;
  const canContinue = selectedCount >= 3;
  const progressLabel = useMemo(() => `第 ${step + 1} 步，共 3 步`, [step]);

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
        {[0, 1, 2].map((index) => (
          <span key={index} className={index <= step ? "active" : ""} />
        ))}
      </div>

      {step === 0 ? (
        <section className="welcome-step">
          <span className="eyebrow">欢迎来到 Tell You Why</span>
          <h1>给工作间隙留一点安静的好奇心。</h1>
          <p>每次用 30–90 秒：看到问题，先想一下，再了解为什么。</p>
          <aside className="preview-question">
            <span>今天的问题</span>
            <strong>{previewCard.question}</strong>
          </aside>
          <ul className="privacy-list">
            <li>无需注册或 API Key</li>
            <li>兴趣和阅读记录默认只在本机</li>
            <li>默认不主动提醒</li>
          </ul>
          <button className="primary-button wide" onClick={() => setStep(1)}>
            选择我的兴趣
          </button>
        </section>
      ) : null}

      {step === 1 ? (
        <section className="interest-step">
          <span className="eyebrow">先选至少 3 个</span>
          <h1>你想多看到哪些内容？</h1>
          <p className="supporting">稍后可以随时修改，选择只保存在本机。</p>
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
              disabled={!canContinue}
              onClick={() => {
                if (!canContinue) setError("请至少选择 3 个兴趣。");
                else setStep(2);
              }}
            >
              继续
            </button>
          </div>
        </section>
      ) : null}

      {step === 2 ? (
        <section className="reminder-step">
          <span className="eyebrow">最后一步</span>
          <h1>什么时候提醒你看看？</h1>
          <p className="supporting">
            默认仅手动打开。通知不会直接展开窗口，也不会播放声音。
          </p>
          <div className="radio-stack">
            {(
              [
                ["manual", "仅手动打开", "通过托盘或 Alt + Shift + Y 唤起"],
                ["weekday_once", "工作日每天 1 次", "默认在 11:00 安静提醒"],
                [
                  "weekday_twice",
                  "工作日每天 2 次",
                  "默认在 11:00 和 16:00 提醒",
                ],
              ] as const
            ).map(([value, label, note]) => (
              <label
                key={value}
                className={
                  reminderPreset === value
                    ? "radio-card selected"
                    : "radio-card"
                }
              >
                <input
                  type="radio"
                  name="reminder"
                  value={value}
                  checked={reminderPreset === value}
                  onChange={() => setReminderPreset(value)}
                />
                <span>
                  <strong>{label}</strong>
                  <small>{note}</small>
                </span>
              </label>
            ))}
          </div>
          <div className="onboarding-final-actions">
            <button className="text-button" onClick={() => setStep(1)}>
              返回修改
            </button>
            <button
              className="primary-button"
              disabled={busy}
              onClick={() =>
                void onComplete({
                  selectedTopicIds: selected,
                  customInterests,
                  reminderPreset,
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
