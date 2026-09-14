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
  stopped: "停止执行",
  retry_scheduled: "等待重试",
  tool_retry_scheduled: "重试只读工具",
  verify_textbook: "校验教材版本",
  completion_check: "检查完成条件",
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
              {trace.harness && (
                <div className="review-harness" aria-label="运行策略与验收">
                  <p className="kb-muted">{trace.harness.version}</p>
                  <p>
                    用量预算占用 {trace.harness.charged_tokens.toLocaleString()}{" "}
                    / {trace.harness.policy.max_token_charge.toLocaleString()}
                    {" · "}执行时间预算占用{" "}
                    {(trace.harness.charged_active_ms / 1000).toFixed(1)} /{" "}
                    {(trace.harness.policy.max_active_ms / 1000).toFixed(0)} 秒
                  </p>
                  <p className="kb-muted">
                    预算占用含估算与未确认请求的预留；实际 token
                    用量以上方供应商报告为准。
                  </p>
                  {trace.harness.context && (
                    <p>
                      当前上下文保留{" "}
                      {trace.harness.context.retained_history_messages}{" "}
                      条历史，省略{" "}
                      {trace.harness.context.omitted_history_messages} 条。
                      输入估算单位{" "}
                      {trace.harness.context.input_units.toLocaleString()} /{" "}
                      {trace.harness.context.max_input_units.toLocaleString()}
                      ；完整历史输入为{" "}
                      {trace.harness.context.full_input_units.toLocaleString()}
                      。
                    </p>
                  )}
                  {trace.harness.completion && (
                    <details>
                      <summary>
                        完成条件检查 · 已核对{" "}
                        {trace.harness.completion.verified_submissions} 次提交
                      </summary>
                      <ul className="review-checks">
                        {trace.harness.completion.checks.map((check) => (
                          <li key={check.code}>
                            {check.label}：
                            {
                              {
                                passed: "通过",
                                pending: "待满足",
                                failed: "未通过",
                              }[check.status]
                            }
                          </li>
                        ))}
                      </ul>
                      <p className="kb-muted">
                        已进行 {trace.harness.completion_repairs}{" "}
                        次步骤修正。内容事实正确性待核验。
                      </p>
                    </details>
                  )}
                </div>
              )}
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
