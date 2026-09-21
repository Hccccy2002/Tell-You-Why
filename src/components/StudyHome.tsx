import { useEffect, useState } from "react";
import {
  studyClient,
  type StudyClient,
  type StudyHomeData,
  type StudyDueItem,
  type StudyDoubtItem,
} from "../lib/study";

interface Props {
  refreshToken?: number;
  busy: boolean;
  onOpen: (sessionId?: string) => void;
  onReview?: (item: StudyDueItem) => void;
  onDoubt?: (item: StudyDoubtItem) => void;
  client?: Pick<StudyClient, "home">;
}

export function StudyHome({
  refreshToken,
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
  }, [client, refresh, refreshToken]);
  const saved = data?.active;
  const savedTitle = saved?.last_title?.trim() || saved?.goal;
  const blockedHint = saved ? "先继续或结束上次学习" : undefined;
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
            <span className="study-home-topic" title={savedTitle}>
              {savedTitle}
            </span>
          ) : (
            <small>约 3 分钟 · 按你的反馈调整</small>
          )}
        </span>
        <span aria-hidden="true">→</span>
      </button>
      {data?.goals?.length ? (
        <section className="study-review-home" aria-label="还没完成的目标">
          <strong>还没完成的目标</strong>
          {data.goals.map((item) => (
            <button
              className="study-review-entry"
              disabled={busy || !!saved}
              title={
                blockedHint ? `${item.title} · ${blockedHint}` : item.title
              }
              aria-description={blockedHint}
              key={item.id}
              onClick={() => onOpen(item.id)}
            >
              <strong className="study-home-title">{item.title}</strong>
              <span aria-hidden="true">→</span>
            </button>
          ))}
        </section>
      ) : null}
      {data?.personalization_enabled && data.doubts?.length && onDoubt ? (
        <section className="study-review-home" aria-label="接着解决疑问">
          <strong>上次这个问题还没讲明白</strong>
          {data.doubts.map((item) => (
            <button
              className="study-review-entry"
              disabled={busy || !!saved}
              title={
                blockedHint
                  ? `${item.question} · ${blockedHint}`
                  : item.question
              }
              aria-description={blockedHint}
              key={item.id}
              onClick={() => onDoubt(item)}
            >
              <strong className="study-home-title">{item.question}</strong>
              <span aria-hidden="true">→</span>
            </button>
          ))}
        </section>
      ) : null}
      {data && onReview ? (
        <section className="study-review-home" aria-label="巩固一下">
          <strong>巩固一下</strong>
          {!data.personalization_enabled ? (
            <p>个性化已关闭，暂不根据过往练习安排巩固。</p>
          ) : data.due_count > 0 ? (
            data.due.map((item) => (
              <button
                className="study-review-entry"
                key={item.concept_key}
                disabled={busy || !!saved}
                title={
                  blockedHint ? `${item.title} · ${blockedHint}` : item.title
                }
                aria-description={blockedHint}
                onClick={() => onReview(item)}
              >
                <strong className="study-home-title">{item.title}</strong>
                <span aria-hidden="true">→</span>
              </button>
            ))
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
