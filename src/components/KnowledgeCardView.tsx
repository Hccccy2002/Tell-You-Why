import { useRef, useState } from "react";
import { openSourceUrl } from "../lib/api";
import type { InteractionKind, KnowledgeCard, TrustStatus } from "../types";

const trustLabels: Record<TrustStatus, string> = {
  verified: "已核验",
  source_grounded: "基于来源生成",
  ai_unverified: "AI 生成 · 未经外部核验",
  demo_unreviewed: "演示内容 · 未经人工复核",
};

interface Props {
  card: KnowledgeCard;
  availableCardCount: number;
  busy: boolean;
  canGoPrevious: boolean;
  onReturnHome: () => void;
  onInteraction: (kind: InteractionKind) => Promise<void>;
  onPrevious: () => void;
  onNext: () => Promise<void>;
  onDismiss: () => Promise<void>;
  onMaster: () => Promise<void>;
  onGenerateSameTopic: () => Promise<void>;
  onGenerateRandomTopic: () => Promise<void>;
}

export function KnowledgeCardView({
  card,
  availableCardCount,
  busy,
  canGoPrevious,
  onReturnHome,
  onInteraction,
  onPrevious,
  onNext,
  onDismiss,
  onMaster,
  onGenerateSameTopic,
  onGenerateRandomTopic,
}: Props) {
  const [revealed, setRevealed] = useState(false);
  const [expanded, setExpanded] = useState(false);
  const answerRef = useRef<HTMLDivElement>(null);

  async function reveal() {
    setRevealed(true);
    await onInteraction("revealed");
    requestAnimationFrame(() => answerRef.current?.focus());
  }

  async function expand() {
    setExpanded((value) => !value);
    if (!expanded) await onInteraction("expanded");
  }

  function returnToQuestion() {
    setExpanded(false);
    setRevealed(false);
  }

  return (
    <main className={revealed ? "card-view revealed" : "card-view"}>
      <div className="card-scroll">
        <article
          className="knowledge-card"
          aria-labelledby="knowledge-question"
        >
          <header className="card-topbar">
            <button
              className="card-home-button"
              type="button"
              aria-label="返回主界面"
              title="返回主界面"
              onClick={onReturnHome}
            >
              <span aria-hidden="true">⌂</span>
            </button>
            <span className="topic-pill">{card.topicLabel}</span>
            <span
              className="card-count"
              role="status"
              aria-label={`当前可展示 ${availableCardCount} 张知识卡`}
            >
              可展示 {availableCardCount} 张
            </span>
          </header>

          <section className="question-block">
            <span className="question-label">今天的问题</span>
            <h1 id="knowledge-question" className="question">
              {card.question}
            </h1>

            {!revealed ? (
              <div className="ponder-panel">
                <p>先留一点空白，想好后再看答案。</p>
                <button
                  className="reveal-button"
                  onClick={() => void reveal()}
                  disabled={busy}
                >
                  我想好了，揭晓答案
                </button>
                <span className="keyboard-hint">按 Enter 也可以继续</span>
              </div>
            ) : null}
          </section>

          {revealed ? (
            <div
              className="answer-panel"
              ref={answerRef}
              tabIndex={-1}
              aria-live="polite"
            >
              <span className="section-label">简短答案</span>
              <p className="short-answer">{card.shortAnswer}</p>
              <button
                className="text-button expand-button"
                aria-expanded={expanded}
                onClick={() => void expand()}
              >
                {expanded ? "收起详细解释" : "展开详细解释"}
                <span aria-hidden="true">{expanded ? " ↑" : " ↓"}</span>
              </button>
              {expanded ? (
                <section className="detail-panel" aria-label="详细解释">
                  <p>{card.explanation}</p>
                  {card.whyItMatters ? (
                    <aside>
                      <strong>为什么值得知道</strong>
                      <p>{card.whyItMatters}</p>
                    </aside>
                  ) : null}
                </section>
              ) : null}
              <div className={`trust-badge trust-${card.trustStatus}`}>
                <span aria-hidden="true">●</span>
                {trustLabels[card.trustStatus]}
              </div>
              {card.trustStatus === "ai_unverified" ? (
                <p className="trust-warning">
                  AI 生成，未经外部核验，可能存在错误。
                </p>
              ) : null}
              {card.sourceRefs.length > 0 ? (
                <section className="sources" aria-label="内容来源">
                  <span>来源</span>
                  {card.sourceRefs.map((source) => (
                    <button
                      key={source.url}
                      className="source-link"
                      onClick={() => void openSourceUrl(source.url)}
                    >
                      {source.publisher ?? source.title}
                      <span className="sr-only">（在浏览器中打开）</span>
                    </button>
                  ))}
                </section>
              ) : null}
            </div>
          ) : null}
        </article>
      </div>

      <footer className="card-footer">
        {revealed ? (
          <>
            <button
              className="footer-side-button previous-button"
              disabled={busy}
              onClick={returnToQuestion}
            >
              <span aria-hidden="true">← </span>
              返回
            </button>
            <div className="footer-center">
              <button
                className="random-footer-button"
                disabled={busy}
                onClick={() => void onGenerateRandomTopic()}
              >
                {busy ? "正在生成…" : "再次生成随机领域知识点"}
              </button>
              <button
                className="mastered-button"
                disabled={busy}
                onClick={() => void onMaster()}
              >
                已狠狠涨知识
              </button>
              <button
                className="same-topic-button"
                disabled={busy}
                onClick={() => void onGenerateSameTopic()}
              >
                {busy ? "正在生成…" : "再次生成同领域知识点"}
              </button>
            </div>
            <button
              className="footer-side-button next-button"
              disabled={busy}
              onClick={() => void onNext()}
            >
              下一条
              <span aria-hidden="true"> →</span>
            </button>
          </>
        ) : (
          <>
            <button
              className="footer-side-button previous-button"
              disabled={busy || (!canGoPrevious && availableCardCount > 1)}
              onClick={onPrevious}
            >
              <span aria-hidden="true">← </span>
              上一条
            </button>
            <div className="footer-center">
              <button
                className="dismiss-button"
                disabled={busy}
                onClick={() => void onDismiss()}
              >
                不感兴趣
              </button>
              <button
                className={
                  card.isFavorite
                    ? "favorite-button selected"
                    : "favorite-button"
                }
                disabled={busy}
                aria-pressed={card.isFavorite}
                onClick={() =>
                  void onInteraction(
                    card.isFavorite ? "unfavorited" : "favorited",
                  )
                }
              >
                <span aria-hidden="true">{card.isFavorite ? "★" : "☆"}</span>
                {card.isFavorite ? "已收藏" : "收藏"}
              </button>
            </div>
            <button
              className="footer-side-button next-button"
              disabled={busy}
              onClick={() => void onNext()}
            >
              下一条
              <span aria-hidden="true"> →</span>
            </button>
          </>
        )}
      </footer>
    </main>
  );
}
