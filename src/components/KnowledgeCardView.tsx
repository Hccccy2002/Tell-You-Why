import { useEffect, useRef, useState } from "react";
import ReactMarkdown, { type Components } from "react-markdown";
import remarkBreaks from "remark-breaks";
import remarkGfm from "remark-gfm";
import { friendlyError, openSourceUrl } from "../lib/api";
import type {
  FollowUpResult,
  FollowUpTurn,
  InteractionKind,
  KnowledgeCard,
  TrustStatus,
} from "../types";

const trustLabels: Record<TrustStatus, string> = {
  verified: "已核验",
  source_grounded: "基于来源生成",
  ai_unverified: "AI 生成 · 未经外部核验",
  demo_unreviewed: "演示内容 · 未经人工复核",
};

const providerLabels = {
  deepseek: "DeepSeek",
  kimi: "Kimi",
} satisfies Record<FollowUpResult["providerId"], string>;

interface FollowUpMessage extends FollowUpTurn {
  result?: FollowUpResult;
}

function isSafeMarkdownUrl(value: string | undefined): value is string {
  if (!value) return false;
  try {
    const url = new URL(value);
    return (
      url.protocol === "https:" &&
      Boolean(url.hostname) &&
      !url.username &&
      !url.password
    );
  } catch {
    return false;
  }
}

const markdownComponents: Components = {
  a({ href, children }) {
    if (!isSafeMarkdownUrl(href)) return <span>{children}</span>;
    return (
      <button
        className="follow-up-markdown-link"
        type="button"
        title="在浏览器中打开链接"
        onClick={() => void openSourceUrl(href)}
      >
        {children}
        <span className="sr-only">（在浏览器中打开）</span>
      </button>
    );
  },
  img({ alt }) {
    return <span className="follow-up-image-placeholder">{alt || "图片"}</span>;
  },
};

const markdownAllowedElements = [
  "p",
  "strong",
  "em",
  "del",
  "ul",
  "ol",
  "li",
  "blockquote",
  "code",
  "pre",
  "a",
  "img",
  "h1",
  "h2",
  "h3",
  "h4",
  "h5",
  "h6",
  "hr",
  "br",
  "table",
  "thead",
  "tbody",
  "tr",
  "th",
  "td",
] as const;

function MarkdownAnswer({ children }: { children: string }) {
  return (
    <div className="follow-up-markdown">
      <ReactMarkdown
        remarkPlugins={[remarkGfm, remarkBreaks]}
        components={markdownComponents}
        allowedElements={[...markdownAllowedElements]}
        skipHtml
      >
        {children}
      </ReactMarkdown>
    </div>
  );
}

interface Props {
  card: KnowledgeCard;
  availableCardCount: number;
  busy: boolean;
  canGoPrevious: boolean;
  onReturnHome: () => void;
  onAskFollowUp: (
    question: string,
    history: FollowUpTurn[],
  ) => Promise<FollowUpResult>;
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
  onAskFollowUp,
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
  const [followUpDraft, setFollowUpDraft] = useState("");
  const [followUpThread, setFollowUpThread] = useState<FollowUpMessage[]>([]);
  const [pendingQuestion, setPendingQuestion] = useState<string | null>(null);
  const [followUpLoading, setFollowUpLoading] = useState(false);
  const [followUpError, setFollowUpError] = useState<string | null>(null);
  const answerRef = useRef<HTMLDivElement>(null);
  const followUpInputRef = useRef<HTMLTextAreaElement>(null);
  const followUpThreadRef = useRef<HTMLDivElement>(null);
  const followUpInFlightRef = useRef(false);
  const followUpRequestRef = useRef(0);

  useEffect(() => {
    const input = followUpInputRef.current;
    if (!input) return;
    input.style.height = "auto";
    input.style.height = `${Math.min(input.scrollHeight, 96)}px`;
  }, [followUpDraft]);

  useEffect(() => {
    const thread = followUpThreadRef.current;
    if (thread) thread.scrollTop = thread.scrollHeight;
  }, [followUpLoading, followUpThread, pendingQuestion]);

  async function reveal() {
    setRevealed(true);
    await onInteraction("revealed");
    requestAnimationFrame(() => answerRef.current?.focus());
  }

  async function expand() {
    setExpanded((value) => !value);
    if (!expanded) await onInteraction("expanded");
  }

  async function submitFollowUp() {
    const question = followUpDraft.trim();
    if (!question || busy || followUpInFlightRef.current) return;

    const requestId = followUpRequestRef.current + 1;
    followUpRequestRef.current = requestId;
    followUpInFlightRef.current = true;
    setFollowUpLoading(true);
    setFollowUpError(null);
    setPendingQuestion(question);

    try {
      const history = followUpThread
        .slice(-6)
        .map(({ role, content }) => ({ role, content }));
      const result = await onAskFollowUp(question, history);
      if (followUpRequestRef.current !== requestId) return;
      const answer = result.answer.trim();
      if (!answer) throw new Error("模型暂未返回内容，请稍后再试");
      setFollowUpThread((current) => [
        ...current,
        { role: "user", content: question },
        {
          role: "assistant",
          content: answer,
          result: { ...result, answer },
        },
      ]);
      setFollowUpDraft("");
    } catch (error) {
      if (followUpRequestRef.current === requestId) {
        setFollowUpError(friendlyError(error));
      }
    } finally {
      if (followUpRequestRef.current === requestId) {
        followUpInFlightRef.current = false;
        setPendingQuestion(null);
        setFollowUpLoading(false);
        requestAnimationFrame(() => followUpInputRef.current?.focus());
      }
    }
  }

  function returnToQuestion() {
    setExpanded(false);
    setRevealed(false);
  }

  const cardBusy = busy || followUpLoading;

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
              <section
                className="follow-up-panel"
                aria-labelledby="follow-up-heading"
              >
                <div className="follow-up-heading">
                  <span id="follow-up-heading" className="section-label">
                    继续追问
                  </span>
                  <small>会参考当前卡片，也可以问其他知识</small>
                </div>
                {followUpThread.length > 0 || pendingQuestion ? (
                  <div
                    ref={followUpThreadRef}
                    className="follow-up-thread"
                    role="log"
                    aria-label="追问对话"
                    aria-live="polite"
                  >
                    {followUpThread.map((message, index) => (
                      <article
                        key={`${index}-${message.role}`}
                        className={`follow-up-message ${message.role}`}
                        aria-label={
                          message.role === "user" ? "你的问题" : undefined
                        }
                      >
                        {message.role === "assistant" ? (
                          <>
                            <strong className="follow-up-author">
                              AI 回答
                            </strong>
                            <MarkdownAnswer>{message.content}</MarkdownAnswer>
                          </>
                        ) : (
                          <p>{message.content}</p>
                        )}
                        {message.role === "assistant" && message.result ? (
                          <small className="follow-up-provider">
                            {providerLabels[message.result.providerId]} ·{" "}
                            {message.result.model} · AI 未核验
                            {message.result.switchedFromProviderId
                              ? ` · 已从 ${
                                  providerLabels[
                                    message.result.switchedFromProviderId
                                  ]
                                } 切换`
                              : ""}
                          </small>
                        ) : null}
                      </article>
                    ))}
                    {pendingQuestion ? (
                      <article
                        className="follow-up-message user pending"
                        aria-label="你的问题"
                      >
                        <p>{pendingQuestion}</p>
                      </article>
                    ) : null}
                    {followUpLoading ? (
                      <div className="follow-up-loading" role="status">
                        AI 正在回答…
                      </div>
                    ) : null}
                  </div>
                ) : null}
                {followUpError ? (
                  <p className="follow-up-error" role="alert">
                    {followUpError}
                  </p>
                ) : null}
                <form
                  className="follow-up-form"
                  aria-busy={followUpLoading}
                  onSubmit={(event) => {
                    event.preventDefault();
                    void submitFollowUp();
                  }}
                >
                  <label className="sr-only" htmlFor="follow-up-question">
                    输入追问
                  </label>
                  <textarea
                    id="follow-up-question"
                    ref={followUpInputRef}
                    rows={1}
                    maxLength={500}
                    value={followUpDraft}
                    disabled={cardBusy}
                    placeholder="输入任何想了解的问题…"
                    aria-describedby="follow-up-keyboard-hint"
                    onChange={(event) => {
                      setFollowUpDraft(event.target.value);
                      if (followUpError) setFollowUpError(null);
                    }}
                    onKeyDown={(event) => {
                      if (event.key !== "Enter" || event.shiftKey) return;
                      if (
                        event.nativeEvent.isComposing ||
                        event.nativeEvent.keyCode === 229
                      ) {
                        return;
                      }
                      event.preventDefault();
                      if (!event.repeat)
                        event.currentTarget.form?.requestSubmit();
                    }}
                  />
                  <button
                    className="follow-up-submit"
                    type="submit"
                    disabled={cardBusy || !followUpDraft.trim()}
                  >
                    {followUpLoading ? "正在回答…" : "发送"}
                  </button>
                </form>
                <small
                  id="follow-up-keyboard-hint"
                  className="follow-up-keyboard-hint"
                >
                  Enter 发送 · Shift + Enter 换行
                </small>
              </section>
            </div>
          ) : null}
        </article>
      </div>

      <footer className="card-footer">
        {revealed ? (
          <>
            <button
              className="footer-side-button previous-button"
              disabled={cardBusy}
              onClick={returnToQuestion}
            >
              <span aria-hidden="true">← </span>
              返回
            </button>
            <div className="footer-center">
              <button
                className="random-footer-button"
                disabled={cardBusy}
                onClick={() => void onGenerateRandomTopic()}
              >
                {busy ? "正在生成…" : "再次生成随机领域知识点"}
              </button>
              <button
                className="mastered-button"
                disabled={cardBusy}
                onClick={() => void onMaster()}
              >
                已狠狠涨知识
              </button>
              <button
                className="same-topic-button"
                disabled={cardBusy}
                onClick={() => void onGenerateSameTopic()}
              >
                {busy ? "正在生成…" : "再次生成同领域知识点"}
              </button>
            </div>
            <button
              className="footer-side-button next-button"
              disabled={cardBusy}
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
              disabled={cardBusy || (!canGoPrevious && availableCardCount > 1)}
              onClick={onPrevious}
            >
              <span aria-hidden="true">← </span>
              上一条
            </button>
            <div className="footer-center">
              <button
                className="dismiss-button"
                disabled={cardBusy}
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
                disabled={cardBusy}
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
              disabled={cardBusy}
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
