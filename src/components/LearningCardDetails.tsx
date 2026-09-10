import { useId, useState } from "react";
import type { RagTask } from "../lib/rag";

export function LearningCardDetails({
  card,
  onPage,
}: {
  card: RagTask;
  onPage: (page: number, version: string) => void;
}) {
  const [expanded, setExpanded] = useState<"evidence" | "explanation" | null>(
    null,
  );
  const panelId = useId();
  if (!card.result) return null;
  const citedIds = new Set(
    [...card.result.answer, ...card.result.explanation].flatMap((claim) =>
      claim.citations.map((citation) => citation.evidence_id),
    ),
  );
  const evidence = card.packet.evidence.filter((item) => citedIds.has(item.id));

  return (
    <div className="learning-details">
      <div className="learning-detail-actions">
        <button
          className="text-button"
          aria-expanded={expanded === "evidence"}
          aria-controls={`${panelId}-evidence`}
          onClick={() =>
            setExpanded(expanded === "evidence" ? null : "evidence")
          }
        >
          原文依据{" "}
          <span aria-hidden="true">{expanded === "evidence" ? "−" : "+"}</span>
        </button>
        <button
          className="text-button"
          aria-expanded={expanded === "explanation"}
          aria-controls={`${panelId}-explanation`}
          onClick={() =>
            setExpanded(expanded === "explanation" ? null : "explanation")
          }
        >
          AI 解释{" "}
          <span aria-hidden="true">
            {expanded === "explanation" ? "−" : "+"}
          </span>
        </button>
      </div>
      {expanded === "evidence" && (
        <section
          id={`${panelId}-evidence`}
          className="learning-detail-panel"
          aria-label="原文依据"
        >
          <p className="learning-detail-caption">
            来自《{card.packet.filename}》的原文摘录
          </p>
          {evidence.length ? (
            evidence.map((item) => (
              <div className="learning-evidence" key={item.id}>
                <p className="learning-evidence-source">
                  {item.chapter_path.join(" / ") || "正文"} · 第 {item.page} 页
                </p>
                <blockquote>{item.text}</blockquote>
                <button
                  className="text-button"
                  onClick={() => onPage(item.page, card.packet.version)}
                >
                  查看第 {item.page} 页原文 ↗
                </button>
              </div>
            ))
          ) : (
            <p>这张卡没有保存可查看的原文依据。</p>
          )}
        </section>
      )}
      {expanded === "explanation" && (
        <section
          id={`${panelId}-explanation`}
          className="learning-detail-panel"
          aria-label="AI 解释"
        >
          {card.result.explanation.length ? (
            card.result.explanation.map((claim, index) => (
              <p className="learning-explanation" key={index}>
                {claim.text}
              </p>
            ))
          ) : (
            <p>这张卡暂未保存详细解释，可先查看原文依据。</p>
          )}
          <p className="learning-detail-caption">
            {card.provider} · {card.model}
          </p>
        </section>
      )}
    </div>
  );
}
