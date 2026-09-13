import { useEffect, useRef, useState } from "react";
import { friendlyError } from "../lib/api";
import {
  chapterPath,
  learningNext,
  learningPrevious,
  learningRecord,
  learningReset,
  learningResume,
  learningStart,
  type LearningEvent,
  type LearningStatus,
  type LearningView,
} from "../lib/learning";
import type { Chapter, PdfKnowledgeBase } from "../lib/knowledgeBase";
import { LearningCardDetails } from "./LearningCardDetails";
import { ConfirmationDialog } from "./ConfirmationDialog";
import { RandomLearningCard } from "./RandomLearningCard";

const labels: Record<LearningStatus, string> = {
  new: "未学习",
  learning: "学习中",
  review: "需复习",
  mastered: "已掌握",
};
export function LearningPanel({
  book,
  chapter,
  chapters,
  active,
  onPage,
  onRestoreChapter,
  onChapterChange,
}: {
  book: PdfKnowledgeBase;
  chapter: string;
  chapters: Chapter[];
  active: boolean;
  onPage: (page: number, version: string) => void;
  onRestoreChapter: (chapter: string) => void;
  onChapterChange: (chapter: string) => void;
}) {
  const [view, setView] = useState<LearningView | null>(null);
  const [filter, setFilter] = useState("default");
  const [saving, setBusy] = useState(false);
  const [generating, setGenerating] = useState(false);
  const busy = saving || generating;
  const [error, setError] = useState<string | null>(null);
  const [reset, setReset] = useState(false);
  const [noMoreCards, setNoMoreCards] = useState(false);
  const [undo, setUndo] = useState<string | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const running = useRef(false);
  const live = useRef(true);
  const confirmAttempt = useRef<string | null>(null);
  const restoreChapter = useRef(onRestoreChapter);
  useEffect(() => {
    restoreChapter.current = onRestoreChapter;
  }, [onRestoreChapter]);
  useEffect(() => {
    live.current = true;
    let current = true;
    void learningResume(book.id)
      .then((value) => {
        if (!current) return;
        setView(value);
        if (value.session) {
          setFilter(value.session.filter);
          restoreChapter.current(value.session.chapter || "");
        }
      })
      .catch((e) => {
        if (current) setError(friendlyError(e));
      });
    return () => {
      current = false;
      live.current = false;
    };
  }, [book.id]);

  async function run(action: () => Promise<LearningView>, clearUndo = true) {
    if (running.current) return;
    running.current = true;
    setBusy(true);
    setError(null);
    try {
      const result = await action();
      if (live.current) {
        setView(result);
        if (clearUndo) setUndo(null);
      }
      return result;
    } catch (e) {
      if (live.current) setError(friendlyError(e));
    } finally {
      running.current = false;
      if (live.current) setBusy(false);
    }
  }
  const session = view?.session;
  const presentation =
    session?.cursor != null ? session.history[session.cursor] : null;
  const matches =
    session?.chapter === (chapter || null) && session?.filter === filter;
  function event(
    kind: LearningEvent["kind"],
    status: LearningStatus | null = null,
  ): LearningEvent {
    if (!session || !presentation) throw new Error("请先开始学习");
    return {
      id: crypto.randomUUID(),
      session_id: session.id,
      presentation_id: presentation.id,
      kind,
      status,
      expected_revision: view?.state?.revision || 0,
      undo_event_id: kind === "undo" ? undo : null,
    };
  }
  useEffect(() => {
    if (
      !active ||
      !presentation ||
      presentation.confirmed ||
      !session ||
      running.current ||
      generating ||
      confirmAttempt.current === presentation.id
    )
      return;
    confirmAttempt.current = presentation.id;
    const request: LearningEvent = {
      id: `shown-${presentation.id}`,
      session_id: session.id,
      presentation_id: presentation.id,
      kind: "shown",
      status: null,
      expected_revision: 0,
      undo_event_id: null,
    };
    void run(() => learningRecord(request));
  });
  async function start(relaxed = false) {
    const result = await run(async () => {
      const started = await learningStart({
        kb: book.id,
        chapter: chapter || null,
        chapter_path: chapterPath(chapters, chapter),
        filter,
        relaxed,
      });
      return learningNext(started.session!);
    });
    if (result && live.current) setSettingsOpen(false);
  }
  async function mark(status: LearningStatus) {
    const request = event("status", status);
    const result = await run(() => learningRecord(request), false);
    if (result && live.current) setUndo(request.id);
  }
  async function next() {
    const result = await run(async () => {
      if (!view || !presentation) throw new Error("请先开始学习");
      let current = view;
      if (!presentation.revealed)
        current = await learningRecord(event("skipped"));
      return learningNext(current.session!);
    });
    if (result && live.current && (result.reason || !result.card?.result)) {
      setNoMoreCards(true);
    }
  }
  function resume() {
    void run(async () => {
      confirmAttempt.current = null;
      return learningResume(book.id);
    });
  }
  const hasCard = !!view?.card?.result && !!presentation;
  return (
    <section className="learning-panel" aria-label="随机学习">
      <div className="learning-toolbar">
        <span>
          {view
            ? `今日已学 ${view.summary.today} 张`
            : error
              ? "学习进度暂不可用"
              : "正在读取学习进度…"}
        </span>
        <button
          className="text-button"
          aria-expanded={settingsOpen}
          aria-controls="learning-settings"
          onClick={() => setSettingsOpen((open) => !open)}
        >
          学习设置
        </button>
      </div>
      {settingsOpen && active && (
        <div className="learning-settings" id="learning-settings">
          <div className="learning-settings-fields">
            <label className="kb-field">
              章节范围
              <select
                value={chapter}
                disabled={busy}
                onChange={(e) => onChapterChange(e.target.value)}
              >
                <option value="">全部章节</option>
                {chapters.map((item) => (
                  <option value={item.id} key={item.id}>
                    {item.title}
                  </option>
                ))}
              </select>
            </label>
            <label className="kb-field">
              学习范围筛选
              <select
                value={filter}
                disabled={busy}
                onChange={(e) => setFilter(e.target.value)}
              >
                <option value="default">未学习优先</option>
                <option value="review">只看需复习</option>
                <option value="all">包含已掌握</option>
              </select>
            </label>
          </div>
          <button
            className="secondary-button"
            disabled={busy || !view}
            onClick={() => void start()}
          >
            应用并换一张
          </button>
          {view && (
            <p className="learning-summary" aria-label="本知识库学习统计">
              未学习 {view.summary.new} · 学习中 {view.summary.learning} ·
              需复习 {view.summary.review} · 已掌握 {view.summary.mastered}
            </p>
          )}
          <div className="learning-settings-actions">
            <button className="text-button" disabled={busy} onClick={resume}>
              恢复学习进度
            </button>
            <button
              className="text-button"
              disabled={busy}
              onClick={() => setReset(true)}
            >
              重置学习记录
            </button>
          </div>
        </div>
      )}
      {error && (
        <div role="alert" className="kb-error">
          {error}
          <button className="text-button" disabled={busy} onClick={resume}>
            重新加载学习进度
          </button>
        </div>
      )}
      {saving && (
        <p className="learning-status" role="status">
          正在保存或加载…
        </p>
      )}
      {session && !matches && (
        <div className="learning-scope-notice" role="status">
          <span>学习范围已更改，当前卡片保持不变。</span>
          {!settingsOpen && (
            <button
              className="text-button"
              disabled={busy}
              onClick={() => void start()}
            >
              切换范围
            </button>
          )}
        </div>
      )}
      {view?.reason && (
        <div className="learning-empty" role="status">
          <h3>这一轮先学到这里</h3>
          <p>{view.reason}，可以复习近期卡片，或生成新卡。</p>
          <button
            className="secondary-button"
            disabled={busy}
            onClick={() => void start(true)}
          >
            复习近期卡片
          </button>
        </div>
      )}
      {view && !hasCard && !view.reason && (
        <div className="learning-empty">
          <span className="learning-eyebrow">每天一张，从这本书开始</span>
          <h3>留一点时间，想一个问题。</h3>
          <p>先思考，再看答案。也可以对照原文，听听 AI 的解释。</p>
          <button
            className="primary-button"
            disabled={busy}
            onClick={() => void start()}
          >
            开始学习
          </button>
        </div>
      )}
      {view?.card?.result && presentation && (
        <article
          className="learning-card"
          aria-label="学习卡片"
          aria-busy={busy}
        >
          <header className="learning-card-meta">
            <span>
              {view.card.packet.evidence[0]?.chapter_path.join(" / ") ||
                view.card.packet.chapter_path?.join(" / ") ||
                "整本教材"}
            </span>
            <span className="learning-card-state">
              {labels[view.state?.status || "new"]}
            </span>
          </header>
          <div className="learning-question">
            <span className="learning-eyebrow">想一想</span>
            <h3>{view.card.result.question}</h3>
          </div>
          {!presentation.revealed ? (
            <div className="learning-ponder">
              <p>先想一想，答案就在下一步。</p>
              <button
                className="primary-button"
                disabled={busy || !presentation.confirmed}
                onClick={() =>
                  void run(() => learningRecord(event("revealed")))
                }
              >
                揭晓答案
              </button>
            </div>
          ) : (
            <div className="learning-answer">
              <span className="learning-eyebrow">简短答案</span>
              {view.card.result.answer.map((claim, index) => (
                <p className="learning-short-answer" key={index}>
                  {claim.text}
                </p>
              ))}
              <LearningCardDetails
                key={presentation.id}
                card={view.card}
                onPage={onPage}
              />
              <p className="learning-trust">
                {view.card.result.generation_mode === "llm"
                  ? "AI 生成 · 原文供对照参考"
                  : "AI 生成 · 请结合原文核对"}
              </p>
              <div className="learning-feedback">
                <button
                  className="text-button"
                  aria-pressed={view.state?.status === "mastered"}
                  disabled={busy}
                  onClick={() =>
                    void mark(
                      view.state?.status === "mastered"
                        ? "learning"
                        : "mastered",
                    )
                  }
                >
                  记住了
                </button>
                <button
                  className="text-button"
                  aria-pressed={view.state?.status === "review"}
                  disabled={busy}
                  onClick={() =>
                    void mark(
                      view.state?.status === "review" ? "learning" : "review",
                    )
                  }
                >
                  需复习
                </button>
                {undo && (
                  <button
                    className="text-button"
                    disabled={busy}
                    onClick={() =>
                      void run(() => learningRecord(event("undo")))
                    }
                  >
                    撤销
                  </button>
                )}
              </div>
            </div>
          )}
        </article>
      )}
      <footer className="learning-navigation" aria-label="学习卡片操作">
        <button
          className="text-button"
          disabled={busy || !session || !session.cursor}
          onClick={() => void run(() => learningPrevious(session!))}
        >
          ← 上一张
        </button>
        <RandomLearningCard
          book={book}
          chapter={chapter}
          disabled={busy}
          onBusyChange={setGenerating}
          onSaved={async () => {
            await run(() => learningResume(book.id));
          }}
        />
        <button
          className={presentation?.revealed ? "primary-button" : "text-button"}
          disabled={busy || !presentation?.confirmed || !matches}
          onClick={() => void next()}
        >
          {!presentation || presentation.revealed ? "下一张 →" : "跳过 →"}
        </button>
      </footer>
      {noMoreCards && active && (
        <ConfirmationDialog
          id="learning-no-more-cards"
          variant="notice"
          eyebrow="随机学习"
          title="学习提示"
          confirmLabel="知道了"
          busyLabel="知道了"
          busy={false}
          onCancel={() => setNoMoreCards(false)}
          onConfirm={() => setNoMoreCards(false)}
        >
          <p>暂时没有新的知识卡了哦~请新增一张~</p>
        </ConfirmationDialog>
      )}
      {reset && (
        <ConfirmationDialog
          id="learning-reset"
          title="重置本知识库学习记录？"
          confirmLabel="重置学习记录"
          busyLabel="正在重置…"
          busy={busy}
          onCancel={() => setReset(false)}
          onConfirm={() =>
            void run(async () => {
              const result = await learningReset(book.id);
              setReset(false);
              return result;
            })
          }
        >
          <p>
            清除掌握状态、学习事件、近期避重和本轮进度，保留学习卡片及教材。
          </p>
          {error && <p role="alert">{error}</p>}
        </ConfirmationDialog>
      )}
    </section>
  );
}
