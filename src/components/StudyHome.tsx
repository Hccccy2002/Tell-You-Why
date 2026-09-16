import { useEffect, useState } from "react";
import {
  studyClient,
  type StudyClient,
  type StudyHomeData,
  type StudyDueItem,
  type StudyDoubtItem,
} from "../lib/study";

interface Props {
  busy: boolean;
  onOpen: (sessionId?: string) => void;
  onReview?: (item: StudyDueItem) => void;
  onDoubt?: (item: StudyDoubtItem) => void;
  client?: Pick<StudyClient, "home">;
}

export function StudyHome({
  busy,
  onOpen,
  onReview,
  onDoubt,
  client = studyClient,
}: Props) {
  const [data, setData] = useState<StudyHomeData | null>(null);
  const [error, setError] = useState(false);
  const [refresh, setRefresh] = useState(0);
  useEffect(() => {
    const update = () => {
      if (!document.hidden) setRefresh((v) => v + 1);
    };
    window.addEventListener("focus", update);
    document.addEventListener("visibilitychange", update);
    const timer = window.setInterval(update, 60_000);
    return () => {
      window.removeEventListener("focus", update);
      document.removeEventListener("visibilitychange", update);
      window.clearInterval(timer);
    };
  }, []);
  useEffect(() => {
    let active = true;
    void client
      .home()
      .then((value) => {
        if (active) {
          setData(value);
          setError(false);
        }
      })
      .catch(() => {
        if (active) setError(true);
      });
    return () => {
      active = false;
    };
  }, [client, refresh]);
  const saved = data?.active;
  return (
    <section className="study-home" aria-label="我的短学习">
      <button
        className="study-entry"
        disabled={busy}
        onClick={() => onOpen(saved?.id)}
      >
        <span>
          <strong>{saved ? "继续上次" : "陪我学一会儿"}</strong>
          {saved ? (
            <>
              <span className="study-home-topic">{saved.goal}</span>
              <small>
                {saved.step_count
                  ? `已留下 ${saved.step_count} 步 · ${saved.last_title ?? "学习中"}`
                  : "主题已保存，还没展开内容"}
              </small>
              <small>
                {saved.state === "failed"
                  ? "上次未完成，打开后可查看或重试"
                  : saved.state === "running"
                    ? "学习正在进行，打开查看进度"
                    : "从保存的位置接着看"}
              </small>
            </>
          ) : (
            <small>约 3 分钟 · 按你的反馈调整</small>
          )}
        </span>
        <span aria-hidden="true">→</span>
      </button>
      {data?.personalization_enabled && data.doubts?.length && onDoubt ? (
        <section className="study-review-home" aria-label="接着解决疑问">
          <strong>上次这个问题还没讲明白</strong>
          {data.doubts.map((item) => (
            <button
              className="study-review-entry"
              disabled={busy || !!saved}
              key={item.id}
              onClick={() => onDoubt(item)}
            >
              <strong>{item.question}</strong>
              <small>你上次反馈“还没懂”，可以换一种方式接着讲</small>
              <span>看看这个问题 →</span>
            </button>
          ))}
          {saved ? (
            <small>先继续或结束上次学习，再开始新的疑问跟进。</small>
          ) : null}
        </section>
      ) : null}
      {data && onReview ? (
        <section className="study-review-home" aria-label="巩固一下">
          <div className="study-step-label">
            <strong>巩固一下</strong>
            {data.due_count > 0 ? (
              <span>{data.due_count} 个知识点可回顾</span>
            ) : null}
          </div>
          {!data.personalization_enabled ? (
            <p>个性化已关闭，暂不根据过往练习安排巩固。</p>
          ) : data.due_count > 0 ? (
            <>
              <p>一次只回顾一个知识点，先补讲，再试一道可跳过的小题。</p>
              {data.due.map((item) => (
                <button
                  className="study-review-entry"
                  key={item.concept_key}
                  disabled={busy || !!saved}
                  onClick={() => onReview(item)}
                >
                  <strong>{item.title}</strong>
                  <small>
                    {item.last_correct
                      ? "到了约定的回顾时间，换个情境再试试"
                      : "上次答案与参考答案不一致，换个例子再看看"}
                  </small>
                  <span>巩固这个知识点 →</span>
                </button>
              ))}
              {saved ? <small>先继续或结束上次学习，再开始巩固。</small> : null}
            </>
          ) : (
            <p>
              {data.practice_count === 0
                ? "做完一道可选练习后，这里会留下之后可巩固的知识点。"
                : data.next_due_at
                  ? `暂时没有到期内容，下次回顾：${new Date(data.next_due_at * 1000).toLocaleString("zh-CN", { month: "numeric", day: "numeric", hour: "2-digit", minute: "2-digit" })}。`
                  : "暂时没有到期内容，可以继续探索。"}
            </p>
          )}
        </section>
      ) : null}
      {error ? (
        <p className="study-home-note" role="status">
          暂时没能读取学习进度。
          <button
            className="text-button"
            onClick={() => setRefresh((v) => v + 1)}
          >
            重试读取
          </button>
        </p>
      ) : null}
    </section>
  );
}
