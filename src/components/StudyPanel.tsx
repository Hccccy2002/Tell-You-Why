import { useCallback, useEffect, useRef, useState, type Ref } from "react";
import { friendlyError } from "../lib/api";
import { useAutoHideGuard } from "../lib/autoHideGuard";
import {
  studyClient,
  type StudyClient,
  type StudyFeedback,
  type StudySession,
  type StudyStep,
  type StudyDueItem,
  type StudyQuestion,
  type StudyDoubtItem,
} from "../lib/study";
import type { KnowledgeCard, ProviderSpec, TopicPreference } from "../types";
import "../study.css";
import { ConfirmationDialog } from "./ConfirmationDialog";
import { StudyHighlights } from "./StudyHighlights";
import { StudyGoalProgress } from "./StudyGoalProgress";
import { StudyStartButton } from "./StudyStartButton";
import { useStudyHighlights } from "../lib/useStudyHighlights";

interface Props {
  modelBusy?: boolean;
  exitLabel?: string;
  topics: TopicPreference[];
  providers: ProviderSpec[];
  onExit: () => void;
  onModelSettings: () => void;
  onOpenCard?: (card: KnowledgeCard, expanded: boolean) => void;
  cardExpanded?: boolean;
  focusSourceId?: string | null;
  doubtTarget?: StudyDoubtItem | null;
  client?: StudyClient;
  sessionId?: string | null;
  reviewTarget?: StudyDueItem | null;
  cardTarget?: Pick<KnowledgeCard, "id" | "question" | "topicLabel"> | null;
}
const feedbackLabels: Record<StudyFeedback, string> = {
  continue: "继续",
  understood: "明白了",
  confused: "没看懂",
  easy: "太简单",
  example: "举个例子",
  skip: "跳过练习",
  answer: "已作答",
};
const kindLabels: Record<StudyStep["kind"], string> = {
  concept: "认识一个知识点",
  prerequisite: "先补一点基础",
  example: "换个例子理解",
  deeper: "再深入一点",
  quiz: "一道可选小练习",
};

function QuestionThread({
  questions,
  focusId,
  focusRef,
  onSave,
  isSaved,
  disabled,
  sourceAnchors = false,
  onFeedback,
  canFeedback,
  onClarify,
  canClarify,
}: {
  questions: StudyQuestion[];
  focusId?: string;
  focusRef?: Ref<HTMLElement>;
  onSave?: (id: string) => void;
  isSaved?: (id: string) => boolean;
  disabled?: boolean;
  sourceAnchors?: boolean;
  onFeedback?: (id: string, feedback: "understood" | "unresolved") => void;
  canFeedback?: (q: StudyQuestion) => boolean;
  onClarify?: (id: string) => void;
  canClarify?: (q: StudyQuestion) => boolean;
}) {
  const labels = {
    explanation: "补充解释",
    comparison: "看一下区别",
    example: "换个例子",
    clarification: "先确认你的意思",
  };
  return (
    <>
      {questions.map((q) => (
        <section
          className="study-question-exchange"
          key={q.id}
          id={sourceAnchors ? `study-source-${q.id}` : undefined}
          tabIndex={-1}
          ref={q.id === focusId ? focusRef : undefined}
        >
          <p>
            <strong>你问：</strong>
            {q.question}
          </p>
          {q.clarification_replies?.length ? (
            <p>
              <strong>你补充：</strong>
              {q.clarification_replies.at(-1)?.reply}
            </p>
          ) : null}
          {q.answer ? (
            <>
              <strong>{labels[q.answer.kind]}</strong>
              <p>{q.answer.text}</p>
              <small>
                AI 辅助回答{q.answer.card_id ? " · 参考了已有知识卡" : ""}
                ，内容可能有误。
              </small>
              {onSave ? (
                <button
                  className="text-button"
                  disabled={disabled || isSaved?.(q.id)}
                  onClick={() => onSave(q.id)}
                >
                  {isSaved?.(q.id) ? "已保存到学习收获" : "保存这段回答"}
                </button>
              ) : null}
              {onFeedback && canFeedback?.(q) ? (
                <div
                  className="study-actions"
                  role="group"
                  aria-label="这次回答讲明白了吗"
                >
                  <button
                    disabled={disabled || q.feedback === "understood"}
                    aria-pressed={q.feedback === "understood"}
                    onClick={() => onFeedback(q.id, "understood")}
                  >
                    明白了
                  </button>
                  <button
                    disabled={disabled || q.feedback === "unresolved"}
                    aria-pressed={q.feedback === "unresolved"}
                    onClick={() => onFeedback(q.id, "unresolved")}
                  >
                    还没懂
                  </button>
                </div>
              ) : null}
              {onClarify && canClarify?.(q) ? (
                <button
                  className="text-button"
                  disabled={disabled}
                  onClick={() => onClarify(q.id)}
                >
                  补充卡住的地方
                </button>
              ) : null}
              {q.feedback ? (
                <p className="study-home-note">
                  {q.feedback === "understood"
                    ? "你反馈已明白，不计为掌握成绩。"
                    : "你反馈了“还没懂”，后续会结合这个问题和已讲过的内容继续解释。"}
                </p>
              ) : null}
            </>
          ) : (
            <small>问题已保存，尚未完成回答。</small>
          )}
        </section>
      ))}
    </>
  );
}

export function StudyPanel({
  modelBusy = false,
  exitLabel,
  topics,
  providers,
  onExit,
  onModelSettings,
  client = studyClient,
  sessionId,
  reviewTarget,
  cardTarget,
  onOpenCard,
  cardExpanded = false,
  focusSourceId,
  doubtTarget,
}: Props) {
  const [run, setRun] = useState<StudySession | null>(null);
  const notes = useStudyHighlights(run?.id, undefined, client);
  function focusSource(id: string) {
    const element = document.getElementById(`study-source-${id}`);
    let parent = element?.parentElement;
    while (parent) {
      if (parent instanceof HTMLDetailsElement) parent.open = true;
      parent = parent.parentElement;
    }
    element?.focus();
    element?.scrollIntoView?.({ block: "center" });
  }
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [stopping, setStopping] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [sourceMissing, setSourceMissing] = useState(false);
  const [goal, setGoal] = useState("");
  const [startHint, setStartHint] = useState<string | null>(null);
  const [selected, setSelected] = useState<number | null>(null);
  const [drafting, setDrafting] = useState(false);
  const [reviewDraft, setReviewDraft] = useState(!!reviewTarget);
  const [cardDraft, setCardDraft] = useState(!!cardTarget);
  const [doubtDraft, setDoubtDraft] = useState(!!doubtTarget);
  const [questionDraft, setQuestionDraft] = useState("");
  const [replyToQuestionId, setReplyToQuestionId] = useState<string | null>(
    null,
  );
  const questionRequest = useRef<{
    stepId: string;
    question: string;
    id: string;
    replyToQuestionId: string | null;
  } | null>(null);
  const questionThread = useRef<HTMLElement>(null);
  const [history, setHistory] = useState<StudySession[] | null>(null);
  const [confirmReset, setConfirmReset] = useState(false);
  const mounted = useRef(false);
  const runRef = useRef<StudySession | null>(null);
  const busyRef = useRef(false);
  const stoppingRef = useRef(false);
  const stepHeading = useRef<HTMLHeadingElement>(null);
  const hasModel = providers.some(
    (p) => p.keyConfigured && p.connectionVerified,
  );
  const interests = topics
    .filter((t) => t.enabled && t.selected)
    .sort((a, b) => a.rank - b.rank);
  const step = run?.steps.at(-1);
  const pendingQuestion = run?.questions.find((q) => !q.answer);
  const canCompleteReview =
    !!run?.review_target &&
    !pendingQuestion &&
    run.steps.some((s) => s.quiz && s.feedback);
  useAutoHideGuard(busy || stopping, "study-session");

  const accept = useCallback((next: StudySession) => {
    const current = runRef.current;
    if (current?.id === next.id && current.revision > next.revision) return;
    runRef.current = next;
    if (mounted.current) {
      if (current?.steps.at(-1)?.id !== next.steps.at(-1)?.id) {
        setQuestionDraft("");
        setReplyToQuestionId(null);
      }
      setRun(next);
      setDrafting(false);
      setReviewDraft(false);
      setCardDraft(false);
      setDoubtDraft(false);
    }
  }, []);

  useEffect(() => {
    mounted.current = true;
    let active = true;
    void (sessionId ? client.read(sessionId) : client.latest())
      .then((latest) => {
        if (!active) return;
        runRef.current = latest;
        setRun(latest);
        if (latest && latest.state !== "completed") {
          setReviewDraft(false);
          setCardDraft(false);
          setDoubtDraft(false);
        }
      })
      .catch((e: unknown) => {
        if (active) setError(friendlyError(e));
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => {
      active = false;
      mounted.current = false;
      const current = runRef.current;
      if (current && busyRef.current)
        void client.pause(current.id, false).catch(() => undefined);
    };
  }, [client, sessionId]);

  useEffect(() => {
    const refresh = () => {
      const current = runRef.current;
      if (!current || busyRef.current || stoppingRef.current) return;
      void client
        .read(current.id)
        .then((latest) => {
          if (
            mounted.current &&
            runRef.current?.id === latest.id &&
            latest.revision > runRef.current.revision
          )
            accept(latest);
        })
        .catch((e: unknown) => {
          if (mounted.current) setError(friendlyError(e));
        });
    };
    window.addEventListener("focus", refresh);
    return () => window.removeEventListener("focus", refresh);
  }, [client, accept]);

  useEffect(() => {
    if (run?.state !== "running" || busy) return;
    // A request may still be finishing after navigation; never issue a second one.
    let active = true;
    const timer = window.setInterval(() => {
      void client
        .read(run.id)
        .then((latest) => {
          if (active) accept(latest);
        })
        .catch((e: unknown) => {
          if (active) setError(friendlyError(e));
        });
    }, 1200);
    return () => {
      active = false;
      window.clearInterval(timer);
    };
  }, [client, run?.id, run?.state, busy, accept]);

  useEffect(() => {
    stepHeading.current?.focus();
  }, [step?.id]);

  const answeredQuestionId = run?.questions.filter((q) => q.answer).at(-1)?.id;
  useEffect(() => {
    if (answeredQuestionId) questionThread.current?.focus();
  }, [answeredQuestionId]);

  const loadedRunId = run?.id;
  useEffect(() => {
    // An explicitly opened source takes priority over the latest-answer focus.
    if (loadedRunId && focusSourceId) focusSource(focusSourceId);
  }, [loadedRunId, focusSourceId]);

  async function operate(operation: () => Promise<void>) {
    if (busyRef.current || stoppingRef.current) return;
    busyRef.current = true;
    setBusy(true);
    setError(null);
    try {
      await operation();
    } catch (e) {
      if (mounted.current) setError(friendlyError(e));
    } finally {
      busyRef.current = false;
      if (mounted.current) setBusy(false);
    }
  }
  async function advance(id: string) {
    if (!mounted.current) {
      await client.pause(id, false);
      return;
    }
    const next = await client.continue(id);
    accept(next);
  }
  function start(topic: string, planned = false) {
    void operate(async () => {
      const next = planned
        ? await client.startGoal(topic)
        : await client.start(
            topic,
            run?.state === "completed" && topic === run.next_topic
              ? run.topic
              : topic,
          );
      accept(next);
      if (!mounted.current) return;
      await advance(next.id);
    });
  }
  function feedback(kind: StudyFeedback, answer?: number) {
    if (!run || !step) return;
    void operate(async () => {
      const next = await client.feedback(run.id, step.id, kind, answer);
      accept(next);
      setSelected(null);
      // Let the reader inspect the result before asking for the next step.
      if (kind !== "answer") await advance(next.id);
    });
  }
  function ask() {
    const question = questionDraft.trim();
    if (
      !run?.can_ask ||
      !step ||
      !question ||
      Array.from(question).length > 300
    )
      return;
    void operate(async () => {
      if (
        questionRequest.current?.stepId !== step.id ||
        questionRequest.current.question !== question ||
        questionRequest.current.replyToQuestionId !== replyToQuestionId
      ) {
        questionRequest.current = {
          stepId: step.id,
          question,
          id: crypto.randomUUID(),
          replyToQuestionId,
        };
      }
      const args = [
        run.id,
        step.id,
        question,
        questionRequest.current.id,
      ] as const;
      const next = replyToQuestionId
        ? await client.ask(...args, replyToQuestionId)
        : await client.ask(...args);
      accept(next);
      if (mounted.current) {
        setQuestionDraft("");
        setReplyToQuestionId(null);
      }
      questionRequest.current = null;
      if (next.questions.some((q) => !q.answer)) await advance(next.id);
    });
  }
  function questionFeedback(id: string, feedback: "understood" | "unresolved") {
    if (!run) return;
    void operate(async () => {
      const next = await client.questionFeedback(run.id, id, feedback);
      accept(next);
      if (id === replyToQuestionId && mounted.current)
        setReplyToQuestionId(null);
      if (next.state !== "completed" && next.questions.some((q) => !q.answer))
        await advance(next.id);
    });
  }
  function canQuestionFeedback(question: StudyQuestion) {
    return !run?.questions.some(
      (q) =>
        q.id !== question.id &&
        q.doubt_id &&
        q.doubt_id === question.doubt_id &&
        run.questions.indexOf(q) > run.questions.indexOf(question),
    );
  }
  async function stop(finish: boolean) {
    if (!run || stoppingRef.current) return;
    stoppingRef.current = true;
    setStopping(true);
    setError(null);
    try {
      const next = await client.pause(run.id, finish);
      accept(next);
      if (!finish) onExit();
    } catch (e) {
      setError(friendlyError(e));
    } finally {
      stoppingRef.current = false;
      if (mounted.current) setStopping(false);
    }
  }
  function explore() {
    const topic = interests[0]?.label ?? "生活中的科学";
    start(topic);
  }
  const active = run && run.state !== "completed";
  const working = busy || run?.state === "running";
  const blocked = working || stopping || modelBusy;
  const learningBlocked = blocked || !!pendingQuestion;

  return (
    <main className="study-page" aria-labelledby="study-title">
      <div className="study-topbar">
        <button
          className="text-button"
          onClick={() => (run && active ? void stop(false) : onExit())}
          disabled={stopping}
        >
          ← {exitLabel ?? "返回小窗"}
        </button>
        <span>约 3 分钟 · 随时可停</span>
      </div>
      <header className="study-heading">
        <span className="eyebrow">一点一点，学得明白</span>
        <h1 id="study-title">陪我学一会儿</h1>
        <p>
          {run && !drafting && (active || run.goal_mode)
            ? run.goal
            : "从一个好奇的问题开始，按你的反馈调整下一步。"}
        </p>
      </header>
      {modelBusy && !working ? (
        <p role="status">模型正在处理请求，请稍候。</p>
      ) : null}
      {run?.source_card && !cardDraft && !drafting && !doubtDraft ? (
        <div className="study-origin">
          <p>本次围绕：{run.source_card.title}</p>
          {onOpenCard ? (
            <button
              className="text-button"
              disabled={blocked || sourceMissing}
              onClick={() =>
                void operate(async () => {
                  if (run.state !== "completed")
                    accept(await client.pause(run.id, false));
                  const card = await client.sourceCard(run.id);
                  if (!mounted.current) return;
                  if (card) onOpenCard(card, run.source_expanded ?? false);
                  else setSourceMissing(true);
                })
              }
            >
              回到原卡
            </button>
          ) : null}
          {sourceMissing ? (
            <p role="status">原卡已删除或不存在，仍可在这里回看学习记录。</p>
          ) : null}
        </div>
      ) : null}
      {active && cardTarget && run.source_card?.card_id !== cardTarget.id ? (
        <p className="study-home-note" role="status">
          已找回上次未结束的学习。请先继续或结束它，再从原卡开始新的学习。
        </p>
      ) : null}
      {active && run.doubt_target ? (
        <p className="study-home-note">
          {run.doubt_target.reason ||
            `你上次对“${run.doubt_target.question}”反馈“还没懂”。`}
          本次会结合此前回答和你补充的卡点重新解释。
        </p>
      ) : null}
      {active && doubtTarget && run.doubt_target?.id !== doubtTarget.id ? (
        <p className="study-home-note">
          已恢复未结束的学习；先继续或结束它，再从首页打开这个疑问。
        </p>
      ) : null}
      {!loading && doubtDraft && doubtTarget ? (
        <section className="study-start" aria-label="继续疑问预览">
          <span className="eyebrow">上次这个问题还没讲明白，要继续吗？</span>
          <h2>{doubtTarget.question}</h2>
          <p className="study-reason">
            {doubtTarget.reason ||
              `你上次对“${doubtTarget.question}”反馈“还没懂”。`}
          </p>
          <p>开始后会结合此前解释，换个例子、补基础，或先确认你卡在哪里。</p>
          <p className="study-privacy">
            将把这个问题、相关内容、此前最多三次回答和三次澄清补充发送给所选模型。
          </p>
          <button
            className="primary-button"
            disabled={!hasModel || blocked}
            onClick={() =>
              void operate(async () => {
                const next = await client.startDoubt(doubtTarget.id);
                accept(next);
                if (mounted.current) await advance(next.id);
              })
            }
          >
            接着解决这个问题
          </button>
          {!hasModel ? (
            <button className="text-button" onClick={onModelSettings}>
              配置模型
            </button>
          ) : null}
          <button className="text-button" disabled={blocked} onClick={onExit}>
            {exitLabel ?? "先返回小窗"}
          </button>
        </section>
      ) : null}
      {loading ? <p role="status">正在找回学习进度…</p> : null}
      {error ? (
        <div className="study-error" role="alert">
          {error}
          <button
            className="text-button"
            onClick={() => {
              const id = runRef.current?.id ?? sessionId;
              void (id ? client.read(id) : client.latest())
                .then((latest) => {
                  if (latest) accept(latest);
                  setError(null);
                })
                .catch((e: unknown) => setError(friendlyError(e)));
            }}
          >
            刷新进度
          </button>
        </div>
      ) : null}
      {!loading && reviewDraft && reviewTarget ? (
        <section className="study-start" aria-label="巩固准备">
          <span className="eyebrow">巩固一下 · 一个知识点</span>
          <h2>{reviewTarget.title}</h2>
          <p>
            {reviewTarget.last_correct
              ? "上次这道题与参考答案一致，现在换个情境回顾一下。"
              : "上次答案与参考答案不一致，先换个例子讲清，再试一道小题。"}
          </p>
          <p className="study-privacy">
            开始后，会把这条练习的题目、你的作答及参考解析发送给所选模型。可以跳过练习或随时结束。
          </p>
          <div className="study-actions">
            <button
              className="primary-button"
              disabled={blocked || !hasModel}
              onClick={() => {
                void operate(async () => {
                  const next = await client.startReview(
                    reviewTarget.concept_key,
                  );
                  accept(next);
                  await advance(next.id);
                });
              }}
            >
              开始巩固
            </button>
            <button className="text-button" disabled={blocked} onClick={onExit}>
              稍后再说
            </button>
          </div>
          {!hasModel ? (
            <button className="text-button" onClick={onModelSettings}>
              配置学习模型 →
            </button>
          ) : null}
        </section>
      ) : null}
      {!loading && cardDraft && cardTarget ? (
        <section className="study-start" aria-label="卡片学习准备">
          <span className="eyebrow">围绕这张卡学一会儿</span>
          <h2>{cardTarget.question}</h2>
          <p>从这张卡的知识点开始，可以补基础、举例子，也可以随时停下。</p>
          <p className="study-privacy">
            开始后，这张卡的内容及启用个性化时的相关学习记录会发送给所选模型。卡片内容不代表已经核验。
          </p>
          <div className="study-actions">
            <button
              className="primary-button"
              disabled={blocked || !hasModel}
              onClick={() => {
                void operate(async () => {
                  const next = await client.startCard(
                    cardTarget.id,
                    cardExpanded,
                  );
                  accept(next);
                  await advance(next.id);
                });
              }}
            >
              开始围绕这张卡学习
            </button>
            <button className="text-button" disabled={blocked} onClick={onExit}>
              稍后再说
            </button>
          </div>
          {!hasModel ? (
            <button className="text-button" onClick={onModelSettings}>
              配置学习模型 →
            </button>
          ) : null}
        </section>
      ) : null}
      {!loading &&
      !reviewDraft &&
      !cardDraft &&
      !doubtDraft &&
      (!run || drafting) ? (
        <section className="study-start" aria-label="开始短学习">
          <form
            onSubmit={(e) => {
              e.preventDefault();
              if (goal.trim()) start(goal.trim());
            }}
          >
            <label htmlFor="study-goal">今天想了解什么？</label>
            <input
              id="study-goal"
              value={goal}
              maxLength={80}
              placeholder="例如：为什么需要 DNS？"
              disabled={blocked}
              onChange={(e) => setGoal(e.target.value)}
            />
            <div className="study-chips">
              {interests.slice(0, 4).map((t) => (
                <button
                  type="button"
                  key={t.id}
                  onClick={() => setGoal(t.label)}
                  disabled={blocked}
                >
                  {t.label}
                </button>
              ))}
            </div>
            <div className="study-actions">
              <StudyStartButton
                hint="把目标拆成最多三个小目标，先了解你卡在哪里，再按你的反馈逐步讲解。"
                activeHint={startHint}
                onHintChange={setStartHint}
                className="primary-button"
                type="button"
                disabled={blocked || !hasModel || !goal.trim()}
                onClick={() => start(goal.trim(), true)}
              >
                按目标学习
              </StudyStartButton>
              <StudyStartButton
                hint="围绕你输入的问题直接开始讲解，可以随时追问、要求举例或补充基础。"
                activeHint={startHint}
                onHintChange={setStartHint}
                className="secondary-button"
                type="submit"
                disabled={blocked || !hasModel || !goal.trim()}
              >
                开始学习
              </StudyStartButton>
              <StudyStartButton
                hint="不用输入问题，从你的首选兴趣开始探索；未设置兴趣时，从生活中的科学开始。"
                activeHint={startHint}
                onHintChange={setStartHint}
                type="button"
                className="secondary-button"
                onClick={explore}
                disabled={blocked || !hasModel}
              >
                随便探索
              </StudyStartButton>
            </div>
          </form>
          {!hasModel ? (
            <p>
              先连接一个模型，就能开始陪伴学习。
              <button className="text-button" onClick={onModelSettings}>
                配置学习模型 →
              </button>
            </p>
          ) : null}
        </section>
      ) : null}
      {!loading &&
      !reviewDraft &&
      !cardDraft &&
      !doubtDraft &&
      run &&
      !drafting ? (
        <>
          {run.goal_plan ? (
            <StudyGoalProgress
              key={run.id}
              plan={run.goal_plan}
              disabled={blocked}
              onCheckin={
                run.state === "waiting"
                  ? (reply) => {
                      void operate(async () => {
                        const next = await client.goalCheckin(run.id, reply);
                        accept(next);
                        await advance(next.id);
                      });
                    }
                  : undefined
              }
            />
          ) : null}
          {run.state === "completed" ? (
            <section className="study-summary" aria-label="学习小结">
              <span className="eyebrow">今天先到这里</span>
              <h2>这一小步，已经留下来了</h2>
              {run.summary.topics.length ? (
                <>
                  <p>本次内容</p>
                  <ul>
                    {[...new Set(run.summary.topics)].map((title) => (
                      <li key={title}>{title}</li>
                    ))}
                  </ul>
                </>
              ) : (
                <p>本次还没有展开内容，下次再开始也没关系。</p>
              )}
              <p>
                {run.summary.answered
                  ? `完成 ${run.summary.answered} 道练习，其中 ${run.summary.correct} 道与参考答案一致。`
                  : "本次没有提交练习答案，未评估掌握程度。"}
              </p>
              {run.next_topic ? (
                <p>
                  <strong>下次可以学：</strong>
                  {run.next_topic}
                </p>
              ) : null}
              {run.review_target ? (
                <p>
                  {run.summary.answered
                    ? "已按这次作答安排下次回顾。"
                    : "本次没有提交新答案，原来的巩固安排保留。"}
                </p>
              ) : null}
              {run.can_reopen_goal ? (
                <button
                  className="primary-button"
                  disabled={blocked}
                  onClick={() =>
                    void operate(async () =>
                      accept(await client.reopenGoal(run.id)),
                    )
                  }
                >
                  继续这个目标
                </button>
              ) : run.goal_mode && !run.goal_plan?.finished ? (
                <p>
                  目标进度已保存，本轮执行预算已用完。可以带着仍需巩固的部分开始新目标。
                </p>
              ) : null}
              <button
                className="primary-button"
                disabled={!hasModel || blocked}
                onClick={() => {
                  setGoal(run.next_topic ?? run.goal);
                  setDrafting(true);
                }}
              >
                再学一会儿
              </button>
              <button className="text-button" onClick={onExit}>
                {exitLabel ?? "回到自由浏览"}
              </button>
            </section>
          ) : (
            <>
              {step ? (
                <article
                  className="study-content"
                  key={step.id}
                  id={`study-source-${step.id}`}
                  tabIndex={-1}
                >
                  <div className="study-step-label">
                    <span>{kindLabels[step.kind]}</span>
                    <span>
                      第 {run.steps.length} 步 / 最多 {run.goal_mode ? 5 : 6} 步
                    </span>
                  </div>
                  <h2 ref={stepHeading} tabIndex={-1}>
                    {step.title}
                  </h2>
                  <p className="study-reason">{step.reason}</p>
                  <div className="study-body">{step.text}</div>
                  {step.quiz ? (
                    <div className="study-quiz">
                      <fieldset
                        disabled={learningBlocked || step.feedback != null}
                      >
                        <legend className="sr-only">选择一个答案</legend>
                        {step.quiz.options.map((option, index) => (
                          <label
                            key={index}
                            className={`study-option${step.quiz?.selected === index ? " submitted" : ""}`}
                          >
                            <input
                              type="radio"
                              name={`study-${step.id}`}
                              checked={
                                (step.quiz?.selected ?? selected) === index
                              }
                              onChange={() => setSelected(index)}
                            />
                            <span>{option}</span>
                          </label>
                        ))}
                      </fieldset>
                      {step.quiz.selected != null ? (
                        <div className="study-answer" role="status">
                          <strong>
                            {step.quiz.correct
                              ? "这次答对了"
                              : "一起看一下这个区别"}
                          </strong>
                          <p>{step.quiz.explanation}</p>
                          <small>
                            依据本题 AI 生成的参考答案，不代表已掌握。
                          </small>
                        </div>
                      ) : null}
                      {!step.feedback ? (
                        <div className="study-actions">
                          <button
                            className="primary-button"
                            disabled={learningBlocked || selected == null}
                            onClick={() => feedback("answer", selected!)}
                          >
                            提交答案
                          </button>
                          <button
                            className="text-button"
                            disabled={learningBlocked}
                            onClick={() => feedback("skip")}
                          >
                            跳过练习
                          </button>
                        </div>
                      ) : null}
                    </div>
                  ) : !step.feedback ? (
                    <div
                      className="study-feedback"
                      aria-label="这段内容适合你吗"
                    >
                      <button
                        className="primary-button"
                        disabled={learningBlocked}
                        onClick={() => feedback("continue")}
                      >
                        继续
                      </button>
                      {run.goal_mode ? (
                        <button
                          disabled={learningBlocked}
                          onClick={() => feedback("understood")}
                        >
                          明白了
                        </button>
                      ) : null}
                      {(["confused", "easy", "example"] as const).map(
                        (kind) => (
                          <button
                            disabled={learningBlocked}
                            key={kind}
                            onClick={() => feedback(kind)}
                          >
                            {run.goal_mode && kind === "confused"
                              ? "还没懂"
                              : feedbackLabels[kind]}
                          </button>
                        ),
                      )}
                    </div>
                  ) : (
                    <p className="study-feedback-saved">
                      已记录：{feedbackLabels[step.feedback]}
                    </p>
                  )}
                  <small className="study-origin">
                    AI 辅助讲解{step.card_id ? " · 参考了已有知识卡" : ""}
                    ，内容可能有误。
                  </small>
                  {!step.quiz ? (
                    <button
                      className="text-button"
                      disabled={
                        blocked || notes.busy || notes.saved("step", step.id)
                      }
                      onClick={() => void notes.save("step", step.id)}
                    >
                      {notes.saved("step", step.id)
                        ? "已保存到学习收获"
                        : "保存这段讲解"}
                    </button>
                  ) : null}
                  <section
                    className="study-question-panel"
                    aria-label="问问当前内容"
                  >
                    <div aria-label="本段问答">
                      <QuestionThread
                        focusId={answeredQuestionId}
                        focusRef={questionThread}
                        sourceAnchors
                        onFeedback={questionFeedback}
                        canFeedback={canQuestionFeedback}
                        onClarify={(id) => {
                          setReplyToQuestionId(id);
                          document.getElementById("study-question")?.focus();
                        }}
                        canClarify={(q) =>
                          !!run.can_ask &&
                          q.answer?.kind === "clarification" &&
                          q.feedback !== "understood" &&
                          canQuestionFeedback(q)
                        }
                        onSave={(id) => void notes.save("question", id)}
                        isSaved={(id) => notes.saved("question", id)}
                        disabled={learningBlocked || notes.busy}
                        questions={run.questions.filter(
                          (q) => q.step_id === step.id,
                        )}
                      />
                    </div>
                    {step.quiz && !step.feedback ? (
                      <small>先试着作答，再提问解析；也可以跳过练习。</small>
                    ) : (
                      <form
                        onSubmit={(event) => {
                          event.preventDefault();
                          ask();
                        }}
                      >
                        <label htmlFor="study-question">
                          {replyToQuestionId ? "补充你卡住的地方" : "我想问……"}
                        </label>
                        <textarea
                          id="study-question"
                          value={questionDraft}
                          maxLength={300}
                          rows={2}
                          placeholder={
                            replyToQuestionId
                              ? "例如：我不明白为什么名字不变，地址却可以变。"
                              : "例如：这里的两个概念有什么区别？"
                          }
                          disabled={blocked || !run.can_ask}
                          onChange={(event) =>
                            setQuestionDraft(event.target.value)
                          }
                        />
                        <div className="study-actions">
                          <button
                            className="secondary-button"
                            type="submit"
                            disabled={
                              blocked ||
                              !run.can_ask ||
                              !hasModel ||
                              !questionDraft.trim()
                            }
                          >
                            {replyToQuestionId ? "发送补充" : "发送问题"}
                          </button>
                          {replyToQuestionId ? (
                            <button
                              className="text-button"
                              type="button"
                              disabled={blocked}
                              onClick={() => setReplyToQuestionId(null)}
                            >
                              改问新问题
                            </button>
                          ) : null}
                          <small>
                            {run.questions.length}/{run.question_limit} 个问题 ·
                            回答后仍停留在本段
                          </small>
                        </div>
                        {run.questions.length >= run.question_limit ? (
                          <small>
                            本次提问次数已用完；未解决的疑问会保留，结束后可从首页继续。
                          </small>
                        ) : null}
                      </form>
                    )}
                  </section>
                </article>
              ) : null}
              {working ? (
                <div className="study-progress" role="status">
                  <span className="study-pulse" />
                  {pendingQuestion
                    ? "正在结合当前内容回答你的问题…"
                    : "正在准备适合你的下一步…"}
                  <small>可以稍后继续，已完成的内容会保留。</small>
                </div>
              ) : null}
              {run.error ? (
                <p role="alert" className="study-error">
                  {run.error}
                </p>
              ) : null}
              {(run.can_resume || canCompleteReview || run.can_finish_goal) &&
              !working ? (
                <button
                  className="primary-button study-resume"
                  disabled={stopping}
                  onClick={() => void operate(() => advance(run.id))}
                >
                  {run.can_finish_goal
                    ? "完成本轮并查看小结"
                    : canCompleteReview
                      ? "完成巩固并查看小结"
                      : pendingQuestion
                        ? "继续回答这个问题"
                        : run.state === "failed"
                          ? "重试当前步骤"
                          : "继续上次学习"}
                </button>
              ) : null}
              {!run.can_resume &&
              !canCompleteReview &&
              !run.can_finish_goal &&
              ["failed", "paused", "ready"].includes(run.state) ? (
                <p>本次学习已达到执行上限，可以结束并查看小结。</p>
              ) : null}
              {!hasModel || run.state === "failed" ? (
                <button className="text-button" onClick={onModelSettings}>
                  检查学习模型设置 →
                </button>
              ) : null}
              <div className="study-session-actions">
                <button
                  className="text-button"
                  disabled={stopping}
                  onClick={() => void stop(false)}
                >
                  稍后继续
                </button>
                <button
                  className="text-button"
                  disabled={stopping}
                  onClick={() => void stop(true)}
                >
                  结束并查看小结
                </button>
              </div>
            </>
          )}
          {run.steps.length > (active ? 1 : 0) ? (
            <details className="study-history">
              <summary>回看本次内容（{run.steps.length} 步）</summary>
              {run.steps.map((s) => (
                <section
                  key={s.id}
                  id={
                    active && step?.id === s.id
                      ? undefined
                      : `study-source-${s.id}`
                  }
                  tabIndex={-1}
                >
                  <h3>{s.title}</h3>
                  <p>{s.text}</p>
                  {!s.quiz ? (
                    <button
                      className="text-button"
                      disabled={
                        blocked || notes.busy || notes.saved("step", s.id)
                      }
                      onClick={() => void notes.save("step", s.id)}
                    >
                      {notes.saved("step", s.id)
                        ? "已保存到学习收获"
                        : "保存这段讲解"}
                    </button>
                  ) : null}
                  {s.quiz?.selected != null ? (
                    <p>
                      你的选择：{s.quiz.options[s.quiz.selected]}
                      <br />
                      {s.quiz.explanation}
                    </p>
                  ) : null}
                  {s.feedback ? (
                    <small>{feedbackLabels[s.feedback]}</small>
                  ) : null}
                  <QuestionThread
                    sourceAnchors={!active || s.id !== step?.id}
                    onFeedback={questionFeedback}
                    canFeedback={canQuestionFeedback}
                    onSave={(id) => void notes.save("question", id)}
                    isSaved={(id) => notes.saved("question", id)}
                    disabled={learningBlocked || notes.busy}
                    questions={run.questions.filter((q) => q.step_id === s.id)}
                  />
                </section>
              ))}
            </details>
          ) : null}
          <p className="study-model">
            本次使用 {run.provider === "deepseek" ? "DeepSeek" : "Kimi"} ·{" "}
            {run.model}
          </p>
          <StudyHighlights
            notes={notes}
            disabled={blocked}
            onOpen={(_id, sourceId) => focusSource(sourceId)}
          />
        </>
      ) : null}
      {!loading ? (
        <details
          className="study-history"
          onToggle={(event) => {
            if (event.currentTarget.open)
              void client
                .history()
                .then(setHistory)
                .catch((e: unknown) => setError(friendlyError(e)));
          }}
        >
          <summary>本机学习记录</summary>
          {history == null ? (
            <p>正在读取…</p>
          ) : history.length === 0 ? (
            <p>还没有学习记录。</p>
          ) : (
            history.map((item) => (
              <details key={item.id} className="study-record">
                <summary>
                  {item.goal} ·{" "}
                  {new Date(item.created_at).toLocaleDateString("zh-CN")}
                </summary>
                {item.goal_plan ? (
                  <StudyGoalProgress plan={item.goal_plan} disabled />
                ) : null}
                {item.can_reopen_goal ? (
                  <button
                    disabled={blocked || !!active}
                    title={active ? "先继续或结束当前学习" : undefined}
                    onClick={() =>
                      void operate(async () =>
                        accept(await client.reopenGoal(item.id)),
                      )
                    }
                  >
                    继续这个目标
                  </button>
                ) : null}
                <p>
                  {item.summary.answered
                    ? `提交 ${item.summary.answered} 道练习，${item.summary.correct} 道与参考答案一致。`
                    : "未提交练习，不推断掌握程度。"}
                </p>
                {item.steps.map((s) => (
                  <section key={s.id}>
                    <h3>{s.title}</h3>
                    <p>{s.text}</p>
                    {s.feedback ? (
                      <small>你的反馈：{feedbackLabels[s.feedback]}</small>
                    ) : null}
                    {s.quiz?.selected != null ? (
                      <p>
                        你的选择：{s.quiz.options[s.quiz.selected]}
                        <br />
                        {s.quiz.explanation}
                      </p>
                    ) : null}
                    <QuestionThread
                      questions={item.questions.filter(
                        (q) => q.step_id === s.id,
                      )}
                    />
                  </section>
                ))}
              </details>
            ))
          )}
          <small>
            这里展示最近 20 次学习。相关记录会在启用个性化时用于后续学习。
          </small>
          <button
            className="text-button"
            disabled={blocked}
            onClick={() => {
              setError(null);
              setConfirmReset(true);
            }}
          >
            清除陪伴学习记录
          </button>
        </details>
      ) : null}
      {confirmReset ? (
        <ConfirmationDialog
          id="study-reset"
          title="清除陪伴学习记录？"
          confirmLabel="确认清除"
          busyLabel="正在清除…"
          busy={busy}
          onCancel={() => setConfirmReset(false)}
          onConfirm={() =>
            void operate(async () => {
              await client.reset();
              runRef.current = null;
              setRun(null);
              setHistory([]);
              setGoal("");
              setSelected(null);
              setDrafting(false);
              setConfirmReset(false);
            })
          }
        >
          <p>
            这会清除陪伴学习的会话、练习记录、学习收获和疑问跟进，结束未完成的学习。现有知识卡、收藏和模型设置会保留。
          </p>
          {error ? <p role="alert">{error}</p> : null}
        </ConfirmationDialog>
      ) : null}
    </main>
  );
}
