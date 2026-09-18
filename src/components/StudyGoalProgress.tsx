import { useState } from "react";
import type { StudyGoalPlan } from "../lib/study";

export function StudyGoalProgress({
  plan,
  disabled,
  onCheckin,
}: {
  plan: StudyGoalPlan;
  disabled: boolean;
  onCheckin?: (reply: string) => void;
}) {
  const [reply, setReply] = useState("");
  const awaitingCheckin = plan.checkin_reply == null;
  return (
    <section className="study-goal-progress" aria-label="学习目标进度">
      <details open={awaitingCheckin || undefined}>
        <summary>
          目标进度 · 已讲解 {plan.objectives.filter((o) => o.taught).length}/
          {plan.objectives.length}
        </summary>
        <ol>
          {plan.objectives.map((o) => (
            <li key={o.id}>
              <strong>{o.title}</strong>
              <span>
                {o.verified
                  ? `已完成验证 · ${o.correct ? "本题与参考答案一致" : "仍需巩固"}`
                  : o.needs_help
                    ? "你反馈还没懂 · 仍需巩固"
                    : o.self_reported_understood
                      ? "你反馈已理解 · 待验证"
                      : o.taught
                        ? "已讲解 · 待验证"
                        : "待学习"}
              </span>
              <small>验证时尝试：{o.criterion}</small>
            </li>
          ))}
        </ol>
        <small>
          每轮最多补讲一次、验证一个子目标。反馈已理解不计为掌握，验证只记录本题作答。
        </small>
        {plan.verification_status === "skipped" ? (
          <p>已跳过验证，保留讲解和反馈记录，不推断掌握程度。</p>
        ) : null}
      </details>
      {awaitingCheckin && onCheckin ? (
        <form
          onSubmit={(e) => {
            e.preventDefault();
            if (reply.trim()) onCheckin(reply.trim());
          }}
        >
          <label htmlFor="goal-checkin">{plan.checkin_prompt}</label>
          <textarea
            id="goal-checkin"
            value={reply}
            maxLength={300}
            placeholder="说说你已经知道什么，或卡在哪一步"
            disabled={disabled}
            onChange={(e) => setReply(e.target.value)}
          />
          <div className="study-actions">
            <button
              className="primary-button"
              disabled={disabled || !reply.trim()}
              type="submit"
            >
              按这个卡点开始
            </button>
            <button
              disabled={disabled}
              type="button"
              onClick={() => onCheckin("")}
            >
              先从基础开始
            </button>
          </div>
        </form>
      ) : null}
    </section>
  );
}
