import { useEffect, useState } from "react";
import { friendlyError } from "../lib/api";
import {
  reviewTrace,
  reviewExport,
  type ReviewRun,
  type ReviewTrace,
} from "../lib/reviewAgent";

const labels: Record<string, string> = {
  chat_completion: "模型请求",
  get_learning_progress: "读取学习记录",
  search_textbook: "检索教材",
  read_source: "阅读原文",
  save_review_question: "保存练习题",
  record_quiz_result: "记录答题结果",
  answer_submitted: "用户提交答案",
  created: "创建复习",
  running: "开始执行",
  resumed: "恢复执行",
  recovered: "恢复中断的任务",
  waiting_answer: "等待作答",
  completed: "复习完成",
  paused: "暂停",
  failed: "执行中断",
};
const statuses = {
  started: "进行中",
  succeeded: "完成",
  failed: "失败",
  interrupted: "中断",
};

export function ReviewTracePanel({
  run,
  active,
}: {
  run: ReviewRun;
  active: boolean;
}) {
  const [open, setOpen] = useState(false);
  const [trace, setTrace] = useState<ReviewTrace | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [exporting, setExporting] = useState(false);
  const [exported, setExported] = useState<string | null>(null);
  useEffect(() => {
    if (!open || !active) return;
    let current = true;
    void reviewTrace(run.id)
      .then((value) => {
        if (current) {
          setTrace(value);
          setError(null);
        }
      })
      .catch((reason) => {
        if (current) setError(friendlyError(reason));
      });
    return () => {
      current = false;
    };
  }, [open, active, run]);

  async function download() {
    if (!trace) return;
    setExporting(true);
    setExported(null);
    try {
      setExported(await reviewExport(trace.run.id));
    } catch (reason) {
      setError(friendlyError(reason));
    } finally {
      setExporting(false);
    }
  }

  return (
    <details
      className="review-trace"
      open={open}
      onToggle={(event) => setOpen(event.currentTarget.open)}
    >
      <summary>执行记录</summary>
      {open && (
        <>
          {error && (
            <p role="alert" className="kb-error">
              {error}
            </p>
          )}
          {!trace && !error && <p role="status">正在读取执行记录…</p>}
          {trace && (
            <>
              <div className="review-session-heading">
                <p className="kb-muted">
                  模型请求 {trace.model_requests} 次 · 工具调用{" "}
                  {trace.tool_calls} 次
                </p>
                <button
                  className="text-button"
                  disabled={exporting}
                  onClick={() => void download()}
                >
                  {exporting ? "正在导出…" : "导出 JSON"}
                </button>
              </div>
              {exported && (
                <p role="status" className="kb-muted">
                  已导出到：{exported}
                </p>
              )}
              <p className="kb-muted">
                {trace.reported_total_tokens == null
                  ? "模型用量未返回"
                  : `已报告 ${trace.reported_total_tokens} tokens（${trace.usage_reported_requests}/${trace.model_requests} 次请求）`}
              </p>
              <ol className="review-timeline">
                {trace.events.map((event) => (
                  <li key={event.sequence}>
                    <div className="review-session-heading">
                      <span>{labels[event.name] ?? event.name}</span>
                      <span className="kb-muted">
                        {statuses[event.status]}
                        {event.duration_ms == null
                          ? ""
                          : ` · ${event.duration_ms} ms`}
                      </span>
                    </div>
                    <details>
                      <summary>步骤详情</summary>
                      <p className="kb-muted">
                        {new Date(event.started_at).toLocaleString()} · #
                        {event.sequence}
                      </p>
                      <pre>{JSON.stringify(event.details, null, 2)}</pre>
                      {event.usage && (
                        <p className="kb-muted">
                          输入 {event.usage.prompt_tokens ?? "未知"} / 输出{" "}
                          {event.usage.completion_tokens ?? "未知"} tokens
                        </p>
                      )}
                    </details>
                  </li>
                ))}
              </ol>
            </>
          )}
        </>
      )}
    </details>
  );
}
