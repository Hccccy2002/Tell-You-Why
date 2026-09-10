import { useEffect, useRef, useState } from "react";
import type { PdfKnowledgeBase } from "../lib/knowledgeBase";
import {
  ragGenerate,
  ragPrepareRandom,
  ragProviders,
  type RagProvider,
  type RagTask,
} from "../lib/rag";
import { friendlyError } from "../lib/api";

export function RandomLearningCard({
  book,
  chapter,
  onSaved,
}: {
  book: PdfKnowledgeBase;
  chapter: string;
  onSaved: () => void;
}) {
  const [opened, setOpened] = useState(false);
  const [providers, setProviders] = useState<RagProvider[]>([]);
  const [provider, setProvider] = useState("");
  const [task, setTask] = useState<RagTask | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const running = useRef(false);
  const live = useRef(true);
  useEffect(() => {
    live.current = true;
    return () => {
      live.current = false;
    };
  }, []);
  useEffect(() => {
    if (!opened) return;
    let active = true;
    void ragProviders()
      .then((p) => {
        if (active) {
          setProviders(p);
          setProvider(p[0] ? `${p[0].id}:${p[0].region}` : "");
        }
      })
      .catch((e) => {
        if (active) setError(friendlyError(e));
      });
    return () => {
      active = false;
    };
  }, [opened]);
  async function run(generate: boolean) {
    if (running.current) return;
    running.current = true;
    setBusy(true);
    setError(null);
    try {
      const selected = providers.find(
        (p) => `${p.id}:${p.region}` === provider,
      );
      if (!selected || !book.version) throw new Error("请先配置并测试模型通道");
      if (!generate) setTask(null);
      const result =
        generate && task
          ? await ragGenerate(task.id)
          : await ragPrepareRandom({
              kb: book.id,
              version: book.version,
              chapter: chapter || null,
              provider: selected.id,
              region: selected.region,
            });
      if (live.current) {
        setTask(result);
        if (
          result.state === "completed" &&
          result.result?.status === "answered"
        )
          onSaved();
      }
    } catch (e) {
      if (live.current) setError(friendlyError(e));
    } finally {
      running.current = false;
      if (live.current) setBusy(false);
    }
  }
  return (
    <div className="learning-generator">
      <button
        className="text-button learning-generator-toggle"
        disabled={busy}
        aria-expanded={opened}
        aria-controls="learning-generator-form"
        onClick={() => setOpened((v) => !v)}
      >
        <span aria-hidden="true">{opened ? "−" : "+"}</span> 生成新卡
      </button>
      {opened && (
        <div className="learning-generator-form" id="learning-generator-form">
          <label className="kb-field">
            生成模型
            <select
              value={provider}
              disabled={busy}
              onChange={(e) => {
                setProvider(e.target.value);
                setTask(null);
              }}
            >
              {providers.map((p) => (
                <option
                  key={`${p.id}:${p.region}`}
                  value={`${p.id}:${p.region}`}
                >
                  {p.id} · {p.region} · {p.model}
                </option>
              ))}
            </select>
          </label>
          {!providers.length && (
            <p className="kb-muted">
              请先在模型设置中配置并测试通道。已有卡片仍可离线学习。
            </p>
          )}
          <button
            className="secondary-button"
            disabled={busy || !provider}
            onClick={() => void run(false)}
          >
            {task ? "换一段原文" : "预览原文"}
          </button>
          {busy && <p role="status">正在处理本次随机学习任务…</p>}
          {error && (
            <p role="alert" className="kb-error">
              {error}
            </p>
          )}
          {task && (
            <article className="rag-result">
              <details open={task.state === "prepared"}>
                <summary>本次教材摘录</summary>
                {task.packet.evidence.map((e) => (
                  <blockquote key={e.id}>
                    <strong>
                      第 {e.page} 页 · {e.chapter_path.join(" / ")}
                    </strong>
                    <p>{e.text}</p>
                  </blockquote>
                ))}
              </details>
              {task.state === "prepared" && (
                <>
                  <p className="kb-muted">
                    将这段原文发送给 {task.provider}{" "}
                    生成卡片和解释，可能产生费用。
                  </p>
                  <button
                    className="primary-button"
                    disabled={busy || !task.packet.evidence.length}
                    onClick={() => void run(true)}
                  >
                    发送摘录并生成新卡
                  </button>
                </>
              )}
              {task.state === "completed" &&
                task.result?.status === "answered" && (
                  <p role="status">
                    {task.duplicate_card
                      ? "题目已存在，保留原卡片和学习状态。"
                      : "新卡已保存为未学习。点击“开始学习”或“下一张”继续。"}
                  </p>
                )}
              {task.result?.status === "insufficient" && (
                <p role="status">依据不足：{task.result.reason}</p>
              )}
              {task.state === "failed" && (
                <p role="alert">
                  {task.error}；本次任务已结束，可重新选择素材，不会自动重试。
                </p>
              )}
              {!!task.usage.length && (
                <details className="learning-generation-info">
                  <summary>生成详情</summary>
                  <p className="kb-muted">
                    {task.provider} · {task.model} · {task.packet.text_chars}{" "}
                    字摘录
                  </p>
                  <p className="kb-muted">
                    已记录 {task.usage.length} 次模型调用 · Token 用量：
                    {task.usage.every(
                      (u) => typeof u.usage?.total_tokens === "number",
                    )
                      ? task.usage.reduce(
                          (sum, u) => sum + (u.usage?.total_tokens || 0),
                          0,
                        )
                      : "未知或不完整"}
                  </p>
                </details>
              )}
            </article>
          )}
        </div>
      )}
    </div>
  );
}
