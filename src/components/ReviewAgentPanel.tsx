import { useEffect, useRef, useState } from "react";
import type { Chapter, PdfKnowledgeBase } from "../lib/knowledgeBase";
import { friendlyError } from "../lib/api";
import { ragProviders } from "../lib/rag";
import {
  reviewAnswer,
  reviewCancel,
  reviewContinue,
  reviewLatest,
  reviewRead,
  reviewStart,
  reviewHistory,
  reviewMemory,
  type ReviewMemoryOverview,
  type ReviewRun,
  type ReviewSummary,
} from "../lib/reviewAgent";
import { ReviewTracePanel } from "./ReviewTracePanel";
import { RelatedSources } from "./RelatedSources";
import "../review-agent.css";

const stateLabels: Record<ReviewRun["state"], string> = {
  ready: "准备就绪",
  running: "正在安排复习…",
  waiting_answer: "轮到你了",
  completed: "本次复习已完成",
  paused: "已暂停",
  failed: "复习暂时中断",
};
export function ReviewAgentPanel({
  book,
  chapters,
  active,
  onPage,
}: {
  book: PdfKnowledgeBase;
  chapters: Chapter[];
  active: boolean;
  onPage: (page: number, version: string) => void;
}) {
  const [goal, setGoal] = useState(
    "结合我的学习记录，选一个需要巩固的知识点，讲解后出题带我复习。",
  );
  const [chapter, setChapter] = useState("");
  const [run, setRun] = useState<ReviewRun | null>(null);
  const [loaded, setLoaded] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [choices, setChoices] = useState<Record<string, number>>({});
  const [sourcesOpen, setSourcesOpen] = useState(false);
  const [sourceQuestionId, setSourceQuestionId] = useState("");
  const sourceQuestion =
    run?.questions.find((q) => `${run.id}/${q.id}` === sourceQuestionId) ??
    run?.questions.at(-1);
  const [history, setHistory] = useState<ReviewSummary[]>([]);
  const [memoryResult, setMemoryResult] = useState<{
    key: string;
    value: ReviewMemoryOverview;
  } | null>(null);
  const memoryKey = `${book.id}/${book.version ?? ""}/${chapter}`;
  const memory = memoryResult?.key === memoryKey ? memoryResult.value : null;
  const [memoryError, setMemoryError] = useState<string | null>(null);
  const live = useRef(true);
  const operating = useRef(false);
  const runId = run?.id;
  const runState = run?.state;
  useEffect(() => {
    live.current = true;
    let current = true;
    void reviewLatest(book.id)
      .then((value) => {
        if (current) {
          setRun(value);
          setLoaded(true);
        }
      })
      .catch((reason) => {
        if (current) {
          setError(friendlyError(reason));
          setLoaded(true);
        }
      });
    return () => {
      current = false;
      live.current = false;
    };
  }, [book.id]);
  useEffect(() => {
    if (!active || runState !== "running" || !runId) return;
    let current = true;
    let timer: ReturnType<typeof setTimeout>;
    async function poll() {
      try {
        const value = await reviewRead(runId!);
        if (current) setRun(value);
      } catch (reason) {
        if (current) setError(friendlyError(reason));
      } finally {
        if (current) timer = setTimeout(() => void poll(), 1500);
      }
    }
    timer = setTimeout(() => void poll(), 1500);
    return () => {
      current = false;
      clearTimeout(timer);
    };
  }, [runId, runState, active]);
  useEffect(() => {
    if (!active) return;
    let current = true;
    void reviewHistory(book.id)
      .then((value) => {
        if (current) setHistory(value);
      })
      .catch((reason) => {
        if (current) setError(friendlyError(reason));
      });
    return () => {
      current = false;
    };
  }, [book.id, active, runId, runState]);
  useEffect(() => {
    if (!active || !book.version) return;
    let current = true;
    const version = book.version;
    async function refreshMemory() {
      try {
        const value = await reviewMemory(book.id, version, chapter || null);
        if (current) {
          setMemoryResult({ key: memoryKey, value });
          setMemoryError(null);
        }
      } catch (reason) {
        if (current) setMemoryError(friendlyError(reason));
      }
    }
    void refreshMemory();
    const timer = setInterval(() => void refreshMemory(), 60000);
    return () => {
      current = false;
      clearInterval(timer);
    };
  }, [book.id, book.version, chapter, active, runId, runState, memoryKey]);

  async function perform(action: () => Promise<ReviewRun>) {
    if (operating.current) return;
    operating.current = true;
    setBusy(true);
    setError(null);
    try {
      const value = await action();
      if (live.current) setRun(value);
    } catch (reason) {
      if (live.current) setError(friendlyError(reason));
    } finally {
      operating.current = false;
      if (live.current) setBusy(false);
    }
  }
  async function continueRun(value: ReviewRun) {
    if (live.current) setRun({ ...value, state: "running" });
    try {
      return await reviewContinue(value.id);
    } catch (reason) {
      // A rejected command may leave a ready/paused checkpoint. Read its real state.
      if (live.current) setRun(await reviewRead(value.id));
      throw reason;
    }
  }
  async function start(dueOnly = false) {
    await perform(async () => {
      const providers = await ragProviders();
      const provider = providers[0];
      if (!provider) throw new Error("请先在模型设置中配置并测试模型通道");
      if (!book.version) throw new Error("请等待教材处理完成");
      const value = await reviewStart({
        kb: book.id,
        version: book.version,
        chapter: chapter || null,
        goal: dueOnly
          ? "复习到期知识点，结合原文讲解并通过答题巩固。"
          : goal.trim(),
        provider: provider.id,
        region: provider.region,
        due_only: dueOnly,
      });
      setChoices({});
      return continueRun(value);
    });
  }
  const running = run?.state === "running";
  const locked = busy || running;
  return (
    <section className="review-agent" aria-label="复习 Agent">
      <div className="review-memory" aria-label="长期复习">
        <div className="review-session-heading">
          <div>
            <strong>待复习 {memory?.due_count ?? "—"}</strong>
            <p className="kb-muted">
              {memory
                ? `已练习 ${memory.total} 个知识点 · 待巩固 ${memory.weak_count} 个`
                : "正在读取复习安排…"}
            </p>
          </div>
          <button
            type="button"
            className="secondary-button"
            disabled={!loaded || locked || !memory?.due_count}
            onClick={() => void start(true)}
          >
            复习到期知识点
          </button>
        </div>
        {memoryError && (
          <p role="alert" className="kb-error">
            {memoryError}
          </p>
        )}
        {memory?.total === 0 && (
          <p className="kb-muted">完成一次答题后，这里会自动安排下次复习。</p>
        )}
        {!!memory?.items.length && (
          <details>
            <summary>知识点与复习时间</summary>
            <ul className="review-memory-list">
              {memory.items.map((item) => (
                <li key={item.id}>
                  <strong>{item.topic}</strong>
                  <span>
                    {item.is_due
                      ? "已到复习时间"
                      : `下次：${new Date(item.due_at).toLocaleString()}`}
                  </span>
                  <span>
                    {item.last_correct
                      ? "最近一次答对"
                      : "最近一次答错，优先巩固"}{" "}
                    · 已答 {item.attempts} 次，答对 {item.correct_count} 次
                  </span>
                </li>
              ))}
            </ul>
            <p className="kb-muted">{memory.note}</p>
          </details>
        )}
      </div>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          void start();
        }}
      >
        <label className="kb-field">
          复习目标
          <textarea
            rows={3}
            maxLength={1000}
            value={goal}
            disabled={locked}
            onChange={(event) => setGoal(event.target.value)}
          />
        </label>
        <div className="review-controls">
          <label className="kb-field">
            复习章节范围
            <select
              value={chapter}
              disabled={locked}
              onChange={(event) => setChapter(event.target.value)}
            >
              <option value="">全部章节</option>
              {chapters.map((item) => (
                <option key={item.id} value={item.id}>
                  {item.title}
                </option>
              ))}
            </select>
          </label>
          <button
            className="primary-button"
            disabled={!loaded || locked || !goal.trim()}
          >
            {run ? "开始新的复习" : "开始复习"}
          </button>
        </div>
      </form>
      {history.length > 1 && (
        <label className="kb-field">
          近期复习
          <select
            value={run?.id ?? ""}
            disabled={locked}
            onChange={(event) => {
              const id = event.target.value;
              void perform(async () => {
                const value = await reviewRead(id);
                setChoices({});
                return value;
              });
            }}
          >
            {!run && (
              <option value="" disabled>
                选择一条复习记录
              </option>
            )}
            {history.map((item) => (
              <option key={item.id} value={item.id}>
                {new Date(item.created_at).toLocaleString()} · {item.goal}
              </option>
            ))}
          </select>
        </label>
      )}
      {!loaded && <p role="status">正在恢复复习进度…</p>}
      {error && (
        <p role="alert" className="kb-error">
          {error}
        </p>
      )}
      {run && (
        <div className="review-session">
          <div className="review-session-heading">
            <span role="status">{stateLabels[run.state]}</span>
            {running && (
              <button
                type="button"
                className="text-button"
                disabled={run.cancel_requested}
                onClick={() => {
                  void reviewCancel(run.id)
                    .then(() => {
                      if (live.current)
                        setRun(
                          (current) =>
                            current && { ...current, cancel_requested: true },
                        );
                    })
                    .catch((reason) => setError(friendlyError(reason)));
                }}
              >
                {run.cancel_requested ? "当前步骤结束后暂停…" : "暂停复习"}
              </button>
            )}
          </div>
          <p className="kb-muted">
            {run.scope.chapter_path.join(" / ") || "整本教材"} · {run.goal}
          </p>
          {run.error && <p className="kb-notice">{run.error}</p>}
          {run.questions.map((question) => (
            <article
              className="review-question"
              key={`${run.id}-${question.id}`}
            >
              <h3>{question.question}</h3>
              <p className="kb-muted">
                {question.source_ids.length
                  ? "已引用教材内容 · 可展开相关原文对照"
                  : "模型补充 · 暂无原文依据"}
              </p>
              {question.correct == null ? (
                <>
                  <fieldset
                    disabled={
                      locked ||
                      question.selected_index != null ||
                      run.state !== "waiting_answer"
                    }
                  >
                    <legend className="sr-only">{question.question}</legend>
                    {question.options.map((option, index) => (
                      <label className="review-option" key={index}>
                        <input
                          type="radio"
                          name={`${run.id}-${question.id}`}
                          checked={
                            (question.selected_index ??
                              choices[question.id]) === index
                          }
                          onChange={() =>
                            setChoices((current) => ({
                              ...current,
                              [question.id]: index,
                            }))
                          }
                        />
                        <span>{option}</span>
                      </label>
                    ))}
                  </fieldset>
                  {question.selected_index == null ? (
                    <button
                      className="primary-button"
                      disabled={
                        locked ||
                        run.state !== "waiting_answer" ||
                        choices[question.id] == null
                      }
                      onClick={() =>
                        void perform(async () =>
                          continueRun(
                            await reviewAnswer(
                              run.id,
                              question.id,
                              choices[question.id]!,
                            ),
                          ),
                        )
                      }
                    >
                      提交答案
                    </button>
                  ) : (
                    <p className="kb-muted">答案已提交，正在记录与讲解。</p>
                  )}
                </>
              ) : (
                <>
                  <p className="review-result">
                    {question.correct ? "答对了" : "这个知识点还需要巩固"} ·
                    你的选择：{question.options[question.selected_index!]}
                  </p>
                  <p>参考答案：{question.options[question.correct_index!]}</p>
                  <p className="review-output">{question.explanation}</p>
                </>
              )}
            </article>
          ))}
          {["paused", "failed", "ready"].includes(run.state) && (
            <button
              className="secondary-button"
              disabled={locked || run.model_calls >= 16 || run.tool_calls >= 32}
              onClick={() => void perform(() => continueRun(run))}
            >
              继续复习
            </button>
          )}
          <details
            className="review-sources"
            onToggle={(event) => setSourcesOpen(event.currentTarget.open)}
          >
            <summary>相关原文 · Top 5</summary>
            {run.questions.length > 1 && (
              <label className="review-source-question">
                对应题目
                <select
                  aria-label="原文对应题目"
                  value={sourceQuestion ? `${run.id}/${sourceQuestion.id}` : ""}
                  onChange={(event) => setSourceQuestionId(event.target.value)}
                >
                  {run.questions.map((q, index) => (
                    <option key={q.id} value={`${run.id}/${q.id}`}>
                      第 {index + 1} 题 · {q.question}
                    </option>
                  ))}
                </select>
              </label>
            )}
            <RelatedSources
              request={{
                kb: run.scope.kb,
                version: run.scope.version,
                chapter: run.scope.chapter,
                query: [...(sourceQuestion?.question || run.goal)]
                  .slice(0, 1000)
                  .join(""),
              }}
              active={active && sourcesOpen}
              onPage={onPage}
            />
          </details>
          <ReviewTracePanel key={run.id} run={run} active={active} />
          <p className="learning-trust">
            AI 辅助复习 · 原文可对照，模型补充内容仅供参考
          </p>
        </div>
      )}
    </section>
  );
}
