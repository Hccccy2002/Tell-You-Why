import { useEffect, useRef, useState } from "react";
import { friendlyError } from "../lib/api";
import {
  ragRelatedSources,
  type EvidencePacket,
  type RelatedSourcesRequest,
} from "../lib/rag";

export function RelatedSources({
  request,
  active,
  onPage,
}: {
  request: RelatedSourcesRequest;
  active: boolean;
  onPage: (page: number, version: string) => void;
}) {
  const { kb, version, chapter, query } = request;
  const key = JSON.stringify([kb, version, chapter, query]);
  const pending = useRef<{
    key: string;
    promise: Promise<EvidencePacket>;
  } | null>(null);
  const [response, setResponse] = useState<{
    key: string;
    packet?: EvidencePacket;
    error?: string;
  } | null>(null);
  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    if (!active) return;
    let live = true;
    if (pending.current?.key !== key) {
      pending.current = {
        key,
        promise: ragRelatedSources({ kb, version, chapter, query }),
      };
    }
    const current = pending.current;
    void current.promise.then(
      (packet) => {
        if (live) setResponse({ key, packet });
      },
      (reason) => {
        if (pending.current === current) pending.current = null;
        if (live) setResponse({ key, error: friendlyError(reason) });
      },
    );
    return () => {
      live = false;
    };
  }, [active, key, kb, version, chapter, query, attempt]);

  if (!active) return null;
  const current = response?.key === key ? response : null;
  if (current?.error)
    return (
      <div>
        <p role="alert" className="kb-error">
          暂时无法读取相关原文：{current.error}
        </p>
        <button
          className="text-button"
          onClick={() => {
            setResponse(null);
            setAttempt((value) => value + 1);
          }}
        >
          重新检索
        </button>
      </div>
    );
  if (!current?.packet)
    return (
      <p role="status" className="kb-muted">
        正在查找与本题最相关的原文…
      </p>
    );
  // Bound rendering too, including responses from an older desktop service.
  const seen = new Set<string>();
  const evidence = current.packet.evidence
    .filter((item) => {
      if (seen.has(item.block_id)) return false;
      seen.add(item.block_id);
      return true;
    })
    .slice(0, 5);
  return (
    <>
      <p className="learning-detail-caption">
        {evidence.length
          ? `按与本题的相关度排序 · ${evidence.length} 段原文`
          : "暂未找到可用的相关原文。"}
      </p>
      {evidence.map((item) => (
        <div className="learning-evidence" key={item.block_id}>
          <p className="learning-evidence-source">
            {item.chapter_path.join(" / ") || "正文"} · 第 {item.page} 页
          </p>
          <blockquote>{item.text}</blockquote>
          <button
            className="text-button"
            onClick={() => onPage(item.page, current.packet!.version)}
          >
            查看第 {item.page} 页原文 ↗
          </button>
        </div>
      ))}
    </>
  );
}
