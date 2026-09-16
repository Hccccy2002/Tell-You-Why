import { useEffect, useRef, useState } from "react";
import { friendlyError } from "../lib/api";
import { kbRead, type PagePreview } from "../lib/knowledgeBase";
import {
  behaviorLabels,
  evaluationBenchmark,
  exportBenchmark,
  kindLabels,
  splitLabels,
  type BenchmarkCase,
  type BenchmarkConfig,
  type BenchmarkDocument,
  type LabelEvidence,
} from "../lib/benchmark";
import type { EvaluationKind } from "../lib/evaluation";

export function BenchmarkPanel({
  kind,
  onChange,
}: {
  kind: EvaluationKind;
  onChange: (value: BenchmarkConfig) => void;
}) {
  const [open, setOpen] = useState(false);
  const [doc, setDoc] = useState<BenchmarkDocument | null>(null);
  const [draft, setDraft] = useState<BenchmarkCase | null>(null);
  const [dirty, setDirty] = useState(false);
  const [confirmed, setConfirmed] = useState(false);
  const [busy, setBusy] = useState(false);
  const pending = useRef(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [page, setPage] = useState(1);
  const [sourceId, setSourceId] = useState("");
  const [blocks, setBlocks] = useState<LabelEvidence[]>([]);
  const [preview, setPreview] = useState<PagePreview | null>(null);
  const [useBenchmark, setUseBenchmark] = useState(false);
  const [releaseId, setReleaseId] = useState("");
  const [split, setSplit] = useState<keyof typeof splitLabels>("regression");
  const [caseText, setCaseText] = useState("");
  const [acknowledged, setAcknowledged] = useState(false);
  const release = doc?.releases.find((r) => r.id === releaseId);
  const available = release?.cases.filter((c) => c.split === split) ?? [];
  const ids = caseText.trim()
    ? caseText.trim().split(/[,，\s]+/)
    : available.map((c) => c.id);
  const valid =
    !!release &&
    ids.length > 0 &&
    new Set(ids).size === ids.length &&
    ids.every((id) => available.some((c) => c.id === id)) &&
    (kind !== "model" || ids.length <= 8) &&
    (split !== "holdout" || acknowledged);
  const selectionKey = ids.join(",");
  useEffect(() => {
    onChange({
      selection: useBenchmark
        ? {
            release: releaseId,
            split,
            case_ids: selectionKey ? selectionKey.split(",") : [],
            acknowledge_holdout: acknowledged,
          }
        : null,
      valid: !useBenchmark || valid,
    });
  }, [
    useBenchmark,
    releaseId,
    split,
    selectionKey,
    acknowledged,
    valid,
    onChange,
  ]);

  async function perform(action: () => Promise<void>) {
    if (pending.current) return;
    pending.current = true;
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      await action();
    } catch (reason) {
      setError(friendlyError(reason));
    } finally {
      pending.current = false;
      setBusy(false);
    }
  }
  function choose(item: BenchmarkCase) {
    setDraft(structuredClone(item));
    setDirty(false);
    setConfirmed(false);
    setPage(item.evidence[0]?.page ?? 1);
    setBlocks([]);
    setPreview(null);
  }
  function edit(patch: Partial<BenchmarkCase>) {
    if (!draft) return;
    setDraft({ ...draft, ...patch });
    setDirty(true);
    setConfirmed(false);
    setNotice(null);
  }
  function reload() {
    void perform(async () => {
      const value = await evaluationBenchmark({ action: "read" });
      setDoc(value);
      const selected =
        value.dataset.cases.find((c) => c.id === draft?.id) ??
        value.dataset.cases[0];
      if (selected) choose(selected);
      else setDraft(null);
      setOpen(true);
    });
  }
  function add() {
    choose({
      id: `q_${crypto.randomUUID().slice(0, 8)}`,
      question: "",
      reference_answer: "",
      acceptance_criteria: "",
      kind: "single",
      split: "development",
      expected_behavior: "answer",
      evidence: [],
      origin: "manual",
      review: {
        method: "unreviewed",
        human_reviewer: null,
        date: null,
        notes: "",
      },
    });
    setDirty(true);
  }
  function save() {
    if (!doc || !draft) return;
    void perform(async () => {
      const value = await evaluationBenchmark({
        action: "save",
        revision: doc.revision,
        case: draft,
        confirm_human: confirmed,
      });
      setDoc(value);
      choose(value.dataset.cases.find((c) => c.id === draft.id)!);
      setNotice(
        confirmed
          ? "标注已由你确认，可封存用于评测。"
          : "题目草稿已保存，尚未计入人工标注基准。",
      );
    });
  }
  return (
    <div className="benchmark-panel">
      <button
        type="button"
        className="text-button"
        disabled={busy}
        onClick={() => (doc ? setOpen(!open) : reload())}
      >
        {open ? "收起人工评测题库" : "管理人工评测题库"}
      </button>
      {error && (
        <p role="alert" className="evaluation-error">
          {error}
        </p>
      )}
      {notice && (
        <p role="status" className="evaluation-note">
          {notice}
        </p>
      )}
      {open && doc && (
        <section aria-label="人工评测题库">
          <h3>先核对标注，再评测模型</h3>
          <label className="evaluation-field">
            选择已导入 PDF
            <select
              disabled={busy || dirty}
              value={sourceId}
              onChange={(e) => setSourceId(e.target.value)}
            >
              <option value="">请选择要标注的资料</option>
              {doc.sources?.map((s) => (
                <option key={s.kb} value={s.kb}>
                  {s.source_filename}
                  {s.status === "partial_ready" ? "（部分内容可检索）" : ""}
                </option>
              ))}
            </select>
          </label>
          <button
            type="button"
            className="secondary-button"
            disabled={busy || dirty || !sourceId}
            onClick={() =>
              void perform(async () => {
                const value = await evaluationBenchmark({
                  action: "bind-source",
                  revision: doc.revision,
                  kb: sourceId,
                });
                setDoc(value);
                setBlocks([]);
                setPreview(null);
                if (value.dataset.cases[0]) choose(value.dataset.cases[0]);
                else {
                  setDraft(null);
                  setDirty(false);
                }
                setNotice(
                  "已切换标注资料。每份 PDF 的草稿分别保留，封存版本保持不变。",
                );
              })
            }
          >
            切换标注资料
          </button>
          {!doc.sources?.length && (
            <p className="evaluation-muted">
              没有可选的已发布资料，请先在 PDF 知识库导入 PDF，再重新载入题库。
            </p>
          )}
          <p className="evaluation-muted">
            {doc.dataset.source_filename} · 共 {doc.dataset.cases.length} 题 ·
            已人工确认{" "}
            {
              doc.dataset.cases.filter((c) => c.review.method === "human")
                .length
            }{" "}
            题。建议积累 50–100
            道经人工核验的问题。历史题只能作为开发或回归题；新资料需要新增题目并核对原文。
          </p>
          <div className="evaluation-actions">
            <button
              className="secondary-button"
              disabled={busy || dirty}
              onClick={add}
            >
              新增题目
            </button>
            <button className="text-button" disabled={busy} onClick={reload}>
              {dirty ? "放弃修改并重新载入" : "重新载入题库"}
            </button>
          </div>
          <label className="evaluation-field">
            选择标注题目
            <select
              disabled={busy || dirty}
              value={draft?.id ?? ""}
              onChange={(e) =>
                choose(doc.dataset.cases.find((c) => c.id === e.target.value)!)
              }
            >
              {draft && !doc.dataset.cases.some((c) => c.id === draft.id) && (
                <option value={draft.id}>新题目 · {draft.id}</option>
              )}
              {doc.dataset.cases.map((c) => (
                <option key={c.id} value={c.id}>
                  {c.review.method === "human" ? "已确认" : "待核对"} · {c.id} ·{" "}
                  {c.question}
                </option>
              ))}
            </select>
          </label>
          {draft && (
            <fieldset disabled={busy} className="benchmark-editor">
              <legend>题目标注 · {draft.id}</legend>
              <label className="evaluation-field">
                问题
                <textarea
                  value={draft.question}
                  maxLength={10000}
                  onChange={(e) => edit({ question: e.target.value })}
                />
              </label>
              <div className="evaluation-judgments">
                <label className="evaluation-field">
                  题型
                  <select
                    value={draft.kind}
                    onChange={(e) =>
                      edit({ kind: e.target.value as BenchmarkCase["kind"] })
                    }
                  >
                    {Object.entries(kindLabels).map(([key, name]) => (
                      <option key={key} value={key}>
                        {name}
                      </option>
                    ))}
                  </select>
                </label>
                <label className="evaluation-field">
                  分组
                  <select
                    value={draft.split}
                    onChange={(e) =>
                      edit({ split: e.target.value as BenchmarkCase["split"] })
                    }
                  >
                    {Object.entries(splitLabels).map(([key, name]) => (
                      <option
                        key={key}
                        value={key}
                        disabled={
                          key === "holdout" && draft.origin === "legacy_exposed"
                        }
                      >
                        {name}
                      </option>
                    ))}
                  </select>
                </label>
                <label className="evaluation-field">
                  预期行为
                  <select
                    value={draft.expected_behavior}
                    onChange={(e) =>
                      edit({
                        expected_behavior: e.target
                          .value as BenchmarkCase["expected_behavior"],
                      })
                    }
                  >
                    {Object.entries(behaviorLabels).map(([key, name]) => (
                      <option key={key} value={key}>
                        {name}
                      </option>
                    ))}
                  </select>
                </label>
              </div>
              <label className="evaluation-field">
                参考答案
                <textarea
                  rows={3}
                  value={draft.reference_answer}
                  maxLength={10000}
                  onChange={(e) => edit({ reference_answer: e.target.value })}
                />
              </label>
              <label className="evaluation-field">
                可接受回答与判定标准
                <textarea
                  rows={3}
                  value={draft.acceptance_criteria}
                  maxLength={10000}
                  placeholder="必须覆盖的要点、允许的表述、错误前提如何纠正，或为什么应当拒答。"
                  onChange={(e) =>
                    edit({ acceptance_criteria: e.target.value })
                  }
                />
              </label>
              <h4>参考证据（PDF 物理页码，从 1 开始）</h4>
              {draft.evidence.map((evidence) => (
                <div className="evaluation-evidence" key={evidence.block_id}>
                  <p>
                    第 {evidence.page} 页 ·{" "}
                    {evidence.eligible_at_annotation
                      ? "已入索引"
                      : "未入索引，仍保留在评测分母"}
                  </p>
                  <blockquote>{evidence.quote}</blockquote>
                  <button
                    type="button"
                    className="text-button"
                    onClick={() =>
                      edit({
                        evidence: draft.evidence.filter(
                          (e) => e.block_id !== evidence.block_id,
                        ),
                      })
                    }
                  >
                    移除此条证据
                  </button>
                </div>
              ))}
              {!draft.evidence.length && (
                <p className="evaluation-muted">
                  暂无证据。可回答题需要至少一条原文；多段题需要至少两条。
                </p>
              )}
              <label className="evaluation-field">
                核对原文页码
                <input
                  type="number"
                  min={1}
                  max={doc.dataset.source_pages}
                  value={page}
                  onChange={(e) => {
                    setPage(Number(e.target.value));
                    setBlocks([]);
                    setPreview(null);
                  }}
                />
              </label>
              <div className="evaluation-actions">
                <button
                  type="button"
                  className="secondary-button"
                  onClick={() =>
                    void perform(async () => {
                      const result = await evaluationBenchmark<{
                        blocks: LabelEvidence[];
                      }>({ action: "source-page", page });
                      setBlocks(result.blocks);
                      if (!result.blocks.length)
                        setNotice("本页没有可用文字块，请查看 PDF 原页核对。");
                    })
                  }
                >
                  读取本页文字
                </button>
                <button
                  type="button"
                  className="secondary-button"
                  onClick={() =>
                    void perform(async () => {
                      setPreview(
                        await kbRead<PagePreview>({
                          op: "page",
                          kb: doc.dataset.kb,
                          version: doc.dataset.knowledge_version,
                          page,
                        }),
                      );
                    })
                  }
                >
                  查看 PDF 原页
                </button>
              </div>
              {preview && (
                <img
                  className="benchmark-page-preview"
                  src={preview.image}
                  alt={`PDF 原页 ${preview.page}`}
                />
              )}
              {blocks.map((block) => (
                <details className="evaluation-evidence" key={block.block_id}>
                  <summary>{block.quote.slice(0, 80)}</summary>
                  <blockquote>{block.quote}</blockquote>
                  <button
                    type="button"
                    className="text-button"
                    disabled={draft.evidence.some(
                      (e) => e.block_id === block.block_id,
                    )}
                    onClick={() =>
                      edit({ evidence: [...draft.evidence, block] })
                    }
                  >
                    加入参考证据
                  </button>
                </details>
              ))}
              <label className="evaluation-field">
                标注复核人
                <input
                  value={draft.review.human_reviewer ?? ""}
                  maxLength={100}
                  onChange={(e) =>
                    edit({
                      review: {
                        ...draft.review,
                        human_reviewer: e.target.value,
                      },
                    })
                  }
                />
              </label>
              <label className="evaluation-field">
                标注核对依据
                <textarea
                  value={draft.review.notes ?? ""}
                  maxLength={5000}
                  placeholder="记录核对的页码、参考答案和证据；拒答题说明资料为何不足。"
                  onChange={(e) =>
                    edit({ review: { ...draft.review, notes: e.target.value } })
                  }
                />
              </label>
              <label className="benchmark-checkbox">
                <input
                  type="checkbox"
                  checked={confirmed}
                  onChange={(e) => {
                    setConfirmed(e.target.checked);
                    setDirty(true);
                  }}
                />
                我已人工核对原 PDF、参考答案和证据，确认此题标注
              </label>
              <p className="evaluation-muted">
                AI
                不能替你完成这项确认。修改后需要重新确认；署名仅是本机记录，不是身份认证。
              </p>
              <button
                type="button"
                className="secondary-button"
                disabled={!dirty}
                onClick={save}
              >
                {confirmed ? "保存并确认标注" : "保存题目草稿"}
              </button>
            </fieldset>
          )}
          {dirty && (
            <p className="evaluation-note">
              题目有未保存的修改。保存后再切换题目或封存。
            </p>
          )}
          <div className="evaluation-actions">
            <button
              className="secondary-button"
              disabled={
                busy ||
                dirty ||
                !doc.dataset.cases.some((c) => c.review.method === "human")
              }
              onClick={() =>
                void perform(async () => {
                  const value = await evaluationBenchmark({
                    action: "freeze",
                    revision: doc.revision,
                  });
                  setDoc(value);
                  setReleaseId(value.releases[0]?.id ?? "");
                  setNotice(
                    "已封存人工确认的题目。后续编辑草稿不会改变该版本或旧报告。",
                  );
                })
              }
            >
              封存已确认题目
            </button>
            <span className="evaluation-muted">
              未确认题目不进入封存版本。封存时校验 PDF 和证据。
            </span>
          </div>
        </section>
      )}
      {doc && kind !== "review" && (
        <div className="benchmark-selection">
          <label className="benchmark-checkbox">
            <input
              type="checkbox"
              checked={useBenchmark}
              onChange={(e) => setUseBenchmark(e.target.checked)}
            />
            本轮使用人工标注基准
          </label>
          {useBenchmark && (
            <>
              <label className="evaluation-field">
                封存版本
                <select
                  value={releaseId}
                  onChange={(e) => {
                    setReleaseId(e.target.value);
                    setCaseText("");
                    setAcknowledged(false);
                  }}
                >
                  <option value="">请选择封存版本</option>
                  {doc.releases.map((r) => (
                    <option key={r.id} value={r.id}>
                      {r.source_filename ?? ""} · {r.version} · {r.cases.length}{" "}
                      题
                    </option>
                  ))}
                </select>
              </label>
              <button
                type="button"
                className="text-button"
                disabled={busy || !release}
                onClick={() =>
                  void perform(async () => {
                    const path = await exportBenchmark(releaseId);
                    if (path) setNotice(`封存题库已导出：${path}`);
                  })
                }
              >
                导出此封存题库
              </button>
              <label className="evaluation-field">
                评测分组
                <select
                  value={split}
                  onChange={(e) => {
                    setSplit(e.target.value as keyof typeof splitLabels);
                    setCaseText("");
                    setAcknowledged(false);
                  }}
                >
                  {Object.entries(splitLabels).map(([key, name]) => (
                    <option key={key} value={key}>
                      {name}
                    </option>
                  ))}
                </select>
              </label>
              <label className="evaluation-field">
                本轮题号（逗号分隔，留空运行本组全部）
                <input
                  value={caseText}
                  onChange={(e) => setCaseText(e.target.value)}
                />
              </label>
              <p className="evaluation-muted">
                可选题号：
                {available.map((c) => c.id).join("、") || "本组暂无题目"}。
                {kind === "model"
                  ? "真实模型每轮最多 8 题、40 次请求，可分批覆盖整个题库；仅运行问答，Agent 流程单独评测。"
                  : "只运行所选分组与题号。"}
              </p>
              {split === "holdout" && (
                <label className="benchmark-checkbox">
                  <input
                    type="checkbox"
                    checked={acknowledged}
                    onChange={(e) => setAcknowledged(e.target.checked)}
                  />
                  方案已固定，本轮留出结果不用于调参；使用记录会保留，重复运行不再视为新盲测
                </label>
              )}
              {!valid && (
                <p className="evaluation-note">
                  请检查封存版本、分组、题号数量及留出确认后再开始。
                </p>
              )}
            </>
          )}
        </div>
      )}
    </div>
  );
}
