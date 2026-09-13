import { useId, useRef, useState } from "react";
import { friendlyError } from "../lib/api";
import type {
  EvaluationDocument,
  HumanReview,
  HumanReviewInput,
  SaveHumanReview,
} from "../lib/evaluation";

export interface ReviewDraft {
  input: HumanReviewInput;
  revision: number;
  dirty: boolean;
}
const judgments = [
  {
    key: "correct",
    label: "事实正确性",
    hint: "回答是否存在事实错误或自相矛盾？",
    yes: "符合：没有事实错误",
    no: "不符合：存在事实错误",
  },
  {
    key: "complete",
    label: "回答完整性",
    hint: "是否回答了问题的关键部分，有无遗漏？",
    yes: "符合：覆盖关键问题",
    no: "不符合：有关键遗漏",
  },
  {
    key: "grounded",
    label: "证据支持情况",
    hint: "原文是否支持回答中的关键结论？证据不足时，是否恰当地说明了限制？",
    yes: "符合：有依据或恰当说明不足",
    no: "不符合：存在无依据的结论",
  },
] as const;

export function EvaluationReviewForm({
  jobId,
  caseId,
  reportHash,
  annotation,
  drafts,
  onSave,
}: {
  jobId: string;
  caseId: string;
  reportHash: string;
  annotation: HumanReview | null;
  drafts: Map<string, ReviewDraft>;
  onSave: (request: SaveHumanReview) => Promise<EvaluationDocument>;
}) {
  const cacheKey = `${jobId}/${reportHash}/${caseId}`;
  const formId = useId();
  const savedDraft: ReviewDraft = {
    input: {
      reviewer: annotation?.reviewer ?? "本机用户",
      correct: annotation?.correct ?? null,
      complete: annotation?.complete ?? null,
      grounded: annotation?.grounded ?? null,
      notes: annotation?.notes ?? "",
    },
    revision: annotation?.revision ?? 0,
    dirty: false,
  };
  const [localDraft, setDraft] = useState<ReviewDraft>(() => {
    const cached = drafts.get(cacheKey);
    return cached?.dirty ? cached : savedDraft;
  });
  const draft = localDraft.dirty ? localDraft : savedDraft;
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const pending = useRef(false);
  const complete =
    judgments.every(({ key }) => draft.input[key] !== null) &&
    !!draft.input.notes.trim();
  function change(patch: Partial<HumanReviewInput>) {
    const next = { ...draft, input: { ...draft.input, ...patch }, dirty: true };
    drafts.set(cacheKey, next);
    setDraft(next);
    setNotice(null);
  }
  async function save() {
    if (pending.current) return;
    pending.current = true;
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const result = await onSave({
        id: jobId,
        case_id: caseId,
        report_sha256: reportHash,
        expected_revision: draft.revision,
        review: draft.input,
      });
      const saved = result.report?.rows.find((r) => r.id === caseId)
        ?.human_review?.annotation;
      if (!saved) throw new Error("未能确认复核已保存，请刷新报告后核对。");
      const next = {
        input: {
          reviewer: saved.reviewer,
          correct: saved.correct,
          complete: saved.complete,
          grounded: saved.grounded,
          notes: saved.notes,
        },
        revision: saved.revision,
        dirty: false,
      };
      drafts.set(cacheKey, next);
      setDraft(next);
      setNotice(
        saved.method === "human"
          ? "复核已保存，质量分数已更新。"
          : "草稿已保存，尚未计入质量分数。",
      );
    } catch (reason) {
      setError(friendlyError(reason));
    } finally {
      pending.current = false;
      setBusy(false);
    }
  }
  return (
    <form
      className="evaluation-review-form"
      aria-label={`人工复核 ${caseId}`}
      onSubmit={(event) => {
        event.preventDefault();
        void save();
      }}
    >
      <h4>人工复核</h4>
      <p className="evaluation-muted">
        请对照上方模型回答和相关原文判断。参考答案仅供参考，也可能有误。
      </p>
      <fieldset disabled={busy}>
        <legend className="sr-only">复核内容</legend>
        <div className="evaluation-judgments">
          {judgments.map(({ key, label, hint, yes, no }) => (
            <div className="evaluation-field" key={key}>
              <label htmlFor={`${formId}-${key}`}>{label}</label>
              <select
                id={`${formId}-${key}`}
                aria-describedby={`${formId}-${key}-hint`}
                value={
                  draft.input[key] === null ? "" : String(draft.input[key])
                }
                onChange={(event) =>
                  change({
                    [key]:
                      event.target.value === ""
                        ? null
                        : event.target.value === "true",
                  })
                }
              >
                <option value="">待判断</option>
                <option value="true">{yes}</option>
                <option value="false">{no}</option>
              </select>
              <span id={`${formId}-${key}-hint`} className="evaluation-muted">
                {hint}
              </span>
            </div>
          ))}
        </div>
        <label className="evaluation-field">
          复核备注
          <textarea
            rows={3}
            maxLength={5000}
            value={draft.input.notes}
            placeholder="说明判断依据，例如遗漏了什么、与哪段原文不符。"
            onChange={(event) => change({ notes: event.target.value })}
          />
        </label>
        <label className="evaluation-field">
          复核人
          <input
            maxLength={100}
            value={draft.input.reviewer}
            onChange={(event) => change({ reviewer: event.target.value })}
          />
        </label>
      </fieldset>
      <p className="evaluation-muted">
        {draft.dirty
          ? "有未保存的修改，请保存后离开页面。"
          : annotation
            ? `上次保存：${new Date(annotation.updated_at).toLocaleString()}`
            : "尚未复核。"}
        {!complete &&
          " 完成三项判断并填写备注后才计入质量分数；也可以先保存草稿。"}
      </p>
      {error && (
        <p role="alert" className="evaluation-error">
          {error} 输入内容已保留。
        </p>
      )}
      {draft.dirty && draft.revision !== (annotation?.revision ?? 0) && (
        <div className="evaluation-note">
          已保存的复核有更新。请核对后重新填写，或放弃本地修改并读取已保存版本。
          <button
            type="button"
            className="text-button"
            disabled={busy}
            onClick={() => {
              drafts.delete(cacheKey);
              setDraft(savedDraft);
              setError(null);
              setNotice(null);
            }}
          >
            放弃修改并读取已保存版本
          </button>
        </div>
      )}
      {notice && (
        <p role="status" className="evaluation-note">
          {notice}
        </p>
      )}
      <button
        className="secondary-button"
        type="submit"
        disabled={busy || !draft.input.reviewer.trim() || !draft.dirty}
      >
        {busy ? "正在保存…" : complete ? "保存复核" : "保存草稿"}
      </button>
    </form>
  );
}
