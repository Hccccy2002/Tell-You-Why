import { useEffect, useRef, useState } from "react";
import { friendlyError } from "../lib/api";
import { useAutoHideGuard } from "../lib/autoHideGuard";
import {
  EvaluationReviewForm,
  type ReviewDraft,
} from "../components/EvaluationReviewForm";
import {
  evaluationCancel,
  evaluationExport,
  evaluationList,
  evaluationRead,
  evaluationSaveReview,
  evaluationStart,
  type EvaluationDocument,
  type EvaluationJob,
  type EvaluationKind,
  type EvaluationReport,
  type SaveHumanReview,
} from "../lib/evaluation";
import { ragProviders, type Evidence, type RagProvider } from "../lib/rag";
import "../evaluation.css";

const suites: {
  id: EvaluationKind;
  title: string;
  description: string;
  badge: string;
}[] = [
  {
    id: "review",
    title: "Agent 流程",
    description: "检查工具调用、答题记录、异常恢复与章节隔离。",
    badge: "10 个场景 · 本地运行",
  },
  {
    id: "top5",
    title: "RAG Top 5",
    description: "对比原检索与重排后的 5 条原文，检查命中率和排名。",
    badge: "40 道教材题 · 本地运行",
  },
  {
    id: "model",
    title: "真实模型",
    description: "生成 8 道教材题的回答，并执行一次完整复习 Agent。",
    badge: "真实 API · 可能产生费用",
  },
];
const stateLabels: Record<EvaluationJob["status"], string> = {
  running: "运行中",
  cancelling: "正在停止",
  completed: "已完成",
  cancelled: "已停止",
  failed: "执行失败",
  interrupted: "已中断",
};
const rowLabels = { passed: "通过", failed: "未通过", unreviewed: "待评估" };
const checkLabels: Record<string, string> = {
  completed: "任务完成",
  tools_used: "必要工具调用",
  actual_answers: "真实答题记录",
  source_ids_valid: "引用编号有效",
  scope_valid: "章节范围正确",
  unique_questions: "无重复题目",
  no_fabricated_citations: "未编造引用",
};
const running = (job: EvaluationJob) =>
  ["running", "cancelling"].includes(job.status);

export function EvaluationScreen() {
  useAutoHideGuard(true, "evaluation-panel");
  const [kind, setKind] = useState<EvaluationKind>("review");
  const [jobs, setJobs] = useState<EvaluationJob[]>([]);
  const [providers, setProviders] = useState<RagProvider[]>([]);
  const [providerKey, setProviderKey] = useState("");
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [document, setDocument] = useState<EvaluationDocument | null>(null);
  const [unavailable, setUnavailable] = useState<string | null>(null);
  const [loaded, setLoaded] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [reportError, setReportError] = useState<string | null>(null);
  const [providerError, setProviderError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [refresh, setRefresh] = useState(0);
  const actionPending = useRef(false);
  const [reviewDrafts] = useState(() => new Map<string, ReviewDraft>());
  const reportReadVersion = useRef(0);
  const requestedReportId = useRef<string | null>(null);
  const live = useRef(true);
  useEffect(() => {
    live.current = true;
    return () => {
      live.current = false;
    };
  }, []);

  useEffect(() => {
    let current = true;
    let timer: ReturnType<typeof setTimeout>;
    async function poll() {
      let active = false;
      try {
        const result = await evaluationList();
        if (current) {
          setJobs(result.jobs);
          setUnavailable(result.unavailable_reason);
          setLoaded(true);
          setLoadError(null);
          active = result.jobs.some(running);
        }
      } catch (reason) {
        if (current) setLoadError(friendlyError(reason));
      } finally {
        if (current)
          timer = setTimeout(() => void poll(), active ? 1200 : 5000);
      }
    }
    void poll();
    return () => {
      current = false;
      clearTimeout(timer);
    };
  }, [refresh]);

  useEffect(() => {
    if (kind !== "model") return;
    let current = true;
    void ragProviders()
      .then((value) => {
        if (current) {
          setProviders(value);
          setProviderError(null);
        }
      })
      .catch((reason) => {
        if (current) setProviderError(friendlyError(reason));
      });
    return () => {
      current = false;
    };
  }, [kind, refresh]);

  const selected = jobs.find((job) => job.id === selectedId) ?? jobs[0];
  const selectedKey = selected?.id;
  const selectedStatus = selected?.status;
  const activeJob = jobs.find(running);
  const provider =
    providers.find((item) => `${item.id}/${item.region}` === providerKey) ??
    providers[0];
  useEffect(() => {
    if (!selectedKey) return;
    let current = true;
    const version = ++reportReadVersion.current;
    requestedReportId.current = selectedKey;
    void evaluationRead(selectedKey)
      .then((value) => {
        if (current && version === reportReadVersion.current) {
          setDocument(value);
          setReportError(null);
        }
      })
      .catch((reason) => {
        if (current && version === reportReadVersion.current)
          setReportError(friendlyError(reason));
      });
    return () => {
      current = false;
    };
  }, [selectedKey, selectedStatus, refresh]);
  const report =
    document && document.job.id === selectedKey ? document.report : null;
  const reportLoading =
    selectedKey && document?.job.id !== selectedKey && !reportError;

  async function perform(action: () => Promise<void>) {
    if (actionPending.current) return;
    actionPending.current = true;
    setBusy(true);
    setNotice(null);
    setError(null);
    try {
      await action();
    } catch (reason) {
      if (live.current) setError(friendlyError(reason));
    } finally {
      actionPending.current = false;
      if (live.current) setBusy(false);
    }
  }
  function start() {
    void perform(async () => {
      const job = await evaluationStart({
        kind,
        ...(kind === "model" && provider
          ? { provider: provider.id, region: provider.region }
          : {}),
      });
      if (!live.current) return;
      setJobs((current) => [
        job,
        ...current.filter((item) => item.id !== job.id),
      ]);
      setSelectedId(job.id);
      setRefresh((value) => value + 1);
    });
  }

  async function saveReview(request: SaveHumanReview) {
    const result = await evaluationSaveReview(request);
    if (live.current) {
      if (requestedReportId.current === result.job.id)
        reportReadVersion.current += 1;
      setDocument((current) =>
        current?.job.id === result.job.id ? result : current,
      );
      setReportError(null);
      setRefresh((value) => value + 1);
    }
    return result;
  }

  return (
    <main className="page-view evaluation-view">
      <div className="evaluation-content">
        <div className="page-heading">
          <span className="eyebrow">用结果检验改进</span>
          <h1>质量评测</h1>
          <p>检查 Agent 流程、原文检索与模型回答。</p>
        </div>
        {(error || loadError) && (
          <div role="alert" className="evaluation-error">
            {error || loadError}
            <button
              className="text-button"
              onClick={() => {
                setError(null);
                setRefresh((value) => value + 1);
              }}
            >
              重试连接
            </button>
          </div>
        )}
        {notice && (
          <p role="status" className="evaluation-note">
            {notice}
          </p>
        )}
        <section className="evaluation-panel" aria-label="新建评测">
          <h2>选择评测</h2>
          <fieldset className="evaluation-suites">
            <legend className="sr-only">评测类型</legend>
            {suites.map((suite) => (
              <label
                key={suite.id}
                className={
                  kind === suite.id
                    ? "evaluation-suite selected"
                    : "evaluation-suite"
                }
              >
                <input
                  type="radio"
                  name="evaluation-kind"
                  value={suite.id}
                  checked={kind === suite.id}
                  onChange={() => setKind(suite.id)}
                />
                <span>
                  <strong>{suite.title}</strong>
                  <span>{suite.description}</span>
                  <small>{suite.badge}</small>
                </span>
              </label>
            ))}
          </fieldset>
          {kind === "model" ? (
            <div className="evaluation-model">
              <label className="evaluation-field">
                模型通道
                <select
                  value={provider ? `${provider.id}/${provider.region}` : ""}
                  onChange={(event) => setProviderKey(event.target.value)}
                >
                  <option value="" disabled>
                    请选择已配置的模型通道
                  </option>
                  {providers.map((item) => (
                    <option
                      key={`${item.id}/${item.region}`}
                      value={`${item.id}/${item.region}`}
                    >
                      {item.id} · {item.model} · {item.region}
                    </option>
                  ))}
                </select>
              </label>
              {providerError && (
                <p role="alert" className="evaluation-error">
                  {providerError}
                </p>
              )}
              {!provider && (
                <p className="evaluation-muted">
                  请先在“模型设置”中配置通道并通过连接测试。
                </p>
              )}
              <p className="evaluation-note">
                开始后会将《计算机组成原理》的检索摘录发送至所选模型的官方
                API，每轮最多 40 次请求，可能产生费用。
              </p>
            </div>
          ) : (
            <p className="evaluation-muted">
              {kind === "review"
                ? "使用固定样例与模拟模型回复，验证真实 Agent 流程。"
                : "使用已建立索引的《计算机组成原理》和本地重排模型，通常需要几分钟。"}
            </p>
          )}
          {unavailable && (
            <p role="alert" className="evaluation-error">
              {unavailable}
            </p>
          )}
          <div className="evaluation-actions">
            <button
              className="primary-button"
              disabled={
                !loaded ||
                busy ||
                !!activeJob ||
                !!unavailable ||
                (kind === "model" && !provider)
              }
              onClick={start}
            >
              {busy
                ? "正在处理…"
                : kind === "model"
                  ? "开始真实模型评测"
                  : "开始评测"}
            </button>
            <span className="evaluation-muted">
              评测记录独立保存，可切换页面。
            </span>
          </div>
        </section>
        {activeJob && (
          <section
            className="evaluation-panel evaluation-progress"
            aria-label="当前评测"
          >
            <div className="evaluation-heading">
              <h2>{activeJob.title}</h2>
              <span className="evaluation-badge">
                {stateLabels[activeJob.status]}
              </span>
            </div>
            <p role="status">{activeJob.message}</p>
            <progress
              value={activeJob.completed}
              max={activeJob.total}
              aria-label="评测进度"
            />
            <div className="evaluation-actions">
              <span>
                {activeJob.completed} / {activeJob.total}
              </span>
              <button
                className="secondary-button"
                disabled={busy || activeJob.status === "cancelling"}
                onClick={() =>
                  void perform(async () => {
                    await evaluationCancel(activeJob.id);
                    if (live.current) setRefresh((value) => value + 1);
                  })
                }
              >
                停止评测
              </button>
              {selected?.id !== activeJob.id && (
                <button
                  className="text-button"
                  onClick={() => setSelectedId(activeJob.id)}
                >
                  查看当前任务
                </button>
              )}
            </div>
          </section>
        )}
        <section className="evaluation-panel" aria-label="评测记录">
          <div className="evaluation-heading">
            <h2>评测记录</h2>
            <button
              className="text-button"
              onClick={() => setRefresh((value) => value + 1)}
            >
              刷新
            </button>
          </div>
          {!loaded ? (
            <p role="status">正在读取评测记录…</p>
          ) : !jobs.length ? (
            <p className="evaluation-muted">
              还没有评测记录。选择一项评测，开始检查当前版本的表现。
            </p>
          ) : (
            <>
              <label className="evaluation-field">
                最近 50 次评测
                <select
                  value={selected?.id ?? ""}
                  onChange={(event) => {
                    setSelectedId(event.target.value);
                    setNotice(null);
                  }}
                >
                  {jobs.map((job) => (
                    <option key={job.id} value={job.id}>
                      {new Date(job.created_at).toLocaleString()} · {job.title}{" "}
                      · {stateLabels[job.status]}
                    </option>
                  ))}
                </select>
              </label>
              {selected && (
                <>
                  <div className="evaluation-heading">
                    <h3>{selected.title}</h3>
                    <span className="evaluation-badge">
                      {stateLabels[selected.status]}
                    </span>
                  </div>
                  {selected.model && (
                    <p className="evaluation-muted">{selected.model}</p>
                  )}
                  {selected.error && (
                    <p role="alert" className="evaluation-error">
                      {selected.error}
                    </p>
                  )}
                  {!running(selected) && selected.status !== "completed" && (
                    <p className="evaluation-muted">
                      {selected.message} · 已处理 {selected.completed} /{" "}
                      {selected.total}
                    </p>
                  )}
                  <button
                    className="secondary-button"
                    disabled={busy || running(selected)}
                    onClick={() =>
                      void perform(async () => {
                        const path = await evaluationExport(selected.id);
                        if (live.current && path)
                          setNotice(`报告已导出到：${path}`);
                      })
                    }
                  >
                    导出 JSON 报告
                  </button>
                </>
              )}
              {reportError && (
                <p role="alert" className="evaluation-error">
                  {reportError}
                </p>
              )}
              {report ? (
                <ReportView
                  key={selectedKey}
                  report={report}
                  job={selected!}
                  drafts={reviewDrafts}
                  onSave={saveReview}
                />
              ) : (
                <p className="evaluation-muted">
                  {reportLoading
                    ? "正在读取评测报告…"
                    : selected && running(selected)
                      ? "任务结束后显示指标与逐题结果。"
                      : "这次任务还没有生成评分报告。"}
                </p>
              )}
            </>
          )}
        </section>
      </div>
    </main>
  );
}

function ReportView({
  report,
  job,
  drafts,
  onSave,
}: {
  report: EvaluationReport;
  job: EvaluationJob;
  drafts: Map<string, ReviewDraft>;
  onSave: (request: SaveHumanReview) => Promise<EvaluationDocument>;
}) {
  const [filter, setFilter] = useState("all");
  const rows = report.rows.filter(
    (row) => filter === "all" || row.status === filter,
  );
  return (
    <div className="evaluation-report">
      <p className="evaluation-muted">数据集：{report.dataset}</p>
      <div className="evaluation-metrics">
        {report.metrics.map((item) => (
          <div key={item.label}>
            <span>{item.label}</span>
            <strong>{item.value}</strong>
            {item.detail && <small>{item.detail}</small>}
            {item.baseline != null && <small>原流程：{item.baseline}</small>}
          </div>
        ))}
      </div>
      {report.notes.map((note) => (
        <p className="evaluation-muted" key={note}>
          {note}
        </p>
      ))}
      {report.human_review?.error ? (
        <p role="alert" className="evaluation-error">
          {report.human_review.error}
        </p>
      ) : (
        report.human_review && (
          <p className="evaluation-note">
            人工复核：{report.human_review.reviewed} /{" "}
            {report.human_review.total} 道可复核问答
            {" · "}已保存草稿：{report.human_review.drafts}{" "}
            道。展开题目即可填写。
          </p>
        )
      )}
      <label className="evaluation-field">
        逐项结果
        <select
          value={filter}
          onChange={(event) => setFilter(event.target.value)}
        >
          <option value="all">全部结果（{report.rows.length}）</option>
          <option value="failed">未通过</option>
          <option value="unreviewed">待评估</option>
          <option value="passed">通过</option>
        </select>
      </label>
      {!rows.length && <p className="evaluation-muted">没有符合条件的结果。</p>}
      <div className="evaluation-cases">
        {rows.map((row) => (
          <details className="evaluation-case" key={row.id}>
            <summary>
              <span>{row.title}</span>
              <span className={`evaluation-result ${row.status}`}>
                {rowLabels[row.status]}
              </span>
            </summary>
            <p className="evaluation-muted">
              {row.id} · {row.note}
            </p>
            <CaseDetails row={row} />
            {row.human_review &&
              report.human_review?.report_sha256 &&
              !report.human_review.error &&
              !running(job) && (
                <EvaluationReviewForm
                  key={report.human_review.report_sha256}
                  jobId={job.id}
                  caseId={row.id}
                  reportHash={report.human_review.report_sha256}
                  annotation={row.human_review.annotation}
                  drafts={drafts}
                  onSave={onSave}
                />
              )}
          </details>
        ))}
      </div>
    </div>
  );
}

function Sources({ sources, title }: { sources: Evidence[]; title: string }) {
  return (
    <details className="evaluation-sources">
      <summary>
        {title}（{sources.length} 条）
      </summary>
      {sources.map((source, index) => (
        <div
          className="evaluation-evidence"
          key={`${source.block_id}-${index}`}
        >
          <p className="evaluation-muted">
            {source.id} · {source.chapter_path?.join(" / ")} · 第 {source.page}{" "}
            页
          </p>
          <blockquote>{source.text}</blockquote>
        </div>
      ))}
    </details>
  );
}

function CaseDetails({ row }: { row: EvaluationReport["rows"][number] }) {
  const details = row.details;
  const run = details.execution?.run ?? details.run;
  const trace = details.execution?.trace ?? details.trace;
  const draft = details.result?.draft;
  return (
    <div className="evaluation-case-content">
      {details.checks && (
        <ul className="evaluation-checks">
          {Object.entries(details.checks).map(([key, passed]) => (
            <li key={key}>
              {passed ? "✓" : "×"} {checkLabels[key] ?? key}
            </li>
          ))}
        </ul>
      )}
      {draft && (
        <>
          <h4>模型回答</h4>
          {draft.status === "insufficient" ? (
            <p>{draft.reason}</p>
          ) : (
            [...draft.answer, ...draft.explanation].map((claim, index) => (
              <div key={index}>
                <p>{claim.text}</p>
                {claim.citations.length > 0 && (
                  <ul className="evaluation-citations">
                    {claim.citations.map((citation, i) => (
                      <li key={i}>
                        {citation.evidence_id} · “{citation.quote}”
                      </li>
                    ))}
                  </ul>
                )}
              </div>
            ))
          )}
        </>
      )}
      {details.reference_answer && (
        <details>
          <summary>参考答案（AI 核验，未人工复核）</summary>
          <p>{details.reference_answer}</p>
        </details>
      )}
      {run?.questions.map((question) => (
        <article key={question.id}>
          <h4>{question.question}</h4>
          <ol>
            {question.options.map((option, index) => (
              <li key={index}>
                {option}
                {question.selected_index === index ? " · 模拟选择" : ""}
                {question.correct_index === index ? " · 模型参考答案" : ""}
              </li>
            ))}
          </ol>
          {question.explanation && <p>{question.explanation}</p>}
        </article>
      ))}
      {details.after && (
        <Sources sources={details.after.sources} title="重排后原文" />
      )}
      {details.before && (
        <Sources sources={details.before.sources} title="原流程原文" />
      )}
      {(details.evidence ?? run?.sources) && (
        <Sources sources={details.evidence ?? run!.sources} title="相关原文" />
      )}
      {trace && (
        <details>
          <summary>执行追踪（{trace.events.length} 步）</summary>
          <ol className="evaluation-trace">
            {trace.events.map((event) => (
              <li key={event.sequence}>
                {event.name} · {event.status}
                {event.duration_ms != null ? ` · ${event.duration_ms} ms` : ""}
              </li>
            ))}
          </ol>
        </details>
      )}
      <details>
        <summary>执行数据（JSON）</summary>
        <pre>{JSON.stringify(details, null, 2)}</pre>
      </details>
    </div>
  );
}
