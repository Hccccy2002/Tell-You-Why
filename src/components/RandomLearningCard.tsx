import { useEffect, useRef, useState } from "react";
import type { PdfKnowledgeBase } from "../lib/knowledgeBase";
import { ragGenerate, ragPrepareRandom, ragProviders } from "../lib/rag";
import { friendlyError } from "../lib/api";

export function RandomLearningCard({
  book,
  chapter,
  disabled = false,
  onBusyChange,
  onSaved,
}: {
  book: PdfKnowledgeBase;
  chapter: string;
  disabled?: boolean;
  onBusyChange?: (busy: boolean) => void;
  onSaved: () => void | Promise<void>;
}) {
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const running = useRef(false);
  const live = useRef(true);
  useEffect(() => {
    live.current = true;
    return () => {
      live.current = false;
    };
  }, []);

  async function generate() {
    if (running.current || disabled) return;
    running.current = true;
    setBusy(true);
    onBusyChange?.(true);
    setError(null);
    setNotice(null);
    try {
      if (!book.version) throw new Error("请等待教材处理完成后再生成新卡");
      const providers = await ragProviders();
      if (!live.current) return;
      const provider = providers[0];
      if (!provider) throw new Error("请先在模型设置中配置并测试模型通道");
      const prepared = await ragPrepareRandom({
        kb: book.id,
        version: book.version,
        chapter: chapter || null,
        provider: provider.id,
        region: provider.region,
      });
      if (!live.current) return;
      if (prepared.state !== "prepared")
        throw new Error(prepared.error || "选材失败，请重新生成");
      const result = await ragGenerate(prepared.id);
      if (!live.current) return;
      if (result.state === "failed")
        throw new Error(result.error || "生成失败，请重试");
      if (result.state !== "completed" || result.result?.status !== "answered")
        throw new Error("本次未生成可用的知识卡，请重试");
      setNotice(
        result.duplicate_card
          ? "题目已存在，保留原卡片和学习状态。"
          : "新卡已保存为未学习。点击“开始学习”或“下一张”继续。",
      );
      await onSaved();
    } catch (e) {
      if (live.current) setError(friendlyError(e));
    } finally {
      running.current = false;
      if (live.current) {
        setBusy(false);
        onBusyChange?.(false);
      }
    }
  }

  return (
    <div className="learning-generator">
      <button
        className="secondary-button learning-generator-button"
        disabled={busy || disabled}
        onClick={() => void generate()}
      >
        {busy ? "生成中…" : "生成新卡"}
      </button>
      {error ? (
        <p role="alert" className="kb-error learning-generation-message">
          {error}
        </p>
      ) : notice ? (
        <p role="status" className="kb-muted learning-generation-message">
          {notice}
        </p>
      ) : null}
    </div>
  );
}
