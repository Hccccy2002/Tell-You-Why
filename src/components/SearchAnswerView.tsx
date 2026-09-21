import { useId, useState } from "react";
import type { SearchAnswer } from "../types";
import { friendlyError, openSourceUrl } from "../lib/api";

export function SearchAnswerView({ answer }: { answer: SearchAnswer }) {
  const [error, setError] = useState("");
  const [open, setOpen] = useState(false);
  const sourcesId = useId();
  return (
    <div className="search-answer">
      <small>
        {answer.status === "answered"
          ? "基于联网资料回答"
          : answer.status === "partial"
            ? "资料仅支持部分回答"
            : "资料不足"}{" "}
        · 查询日期 {answer.asOf}
        {answer.cacheHit ? " · 使用五分钟内缓存" : ""}
      </small>
      {answer.blocks.map((block, index) => (
        <p key={index}>
          {block.text}
          {block.evidenceIds.map((id) => (
            <button
              type="button"
              className="citation-button"
              key={id}
              aria-label={`查看来源 ${id}`}
              aria-controls={sourcesId}
              onClick={() => setOpen(true)}
            >
              [{id}]
            </button>
          ))}
        </p>
      ))}
      {answer.limitation ? (
        <p className="search-limitation">{answer.limitation}</p>
      ) : null}
      {answer.sources.length ? (
        <div className="search-sources">
          <button
            type="button"
            className="search-sources-toggle"
            aria-expanded={open}
            aria-controls={sourcesId}
            onClick={() => setOpen((value) => !value)}
          >
            参考资料
            <span className="search-source-count">{answer.sources.length}</span>
            <svg
              className="search-sources-chevron"
              width="16"
              height="16"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="1.8"
              strokeLinecap="round"
              strokeLinejoin="round"
              aria-hidden="true"
            >
              <path d="m6 9 6 6 6-6" />
            </svg>
          </button>
          <ul id={sourcesId} className="search-source-list" hidden={!open}>
            {answer.sources.map((source) => (
              <li key={source.id}>
                {source.url ? (
                  <a
                    className="search-source-link"
                    href={source.url}
                    target="_blank"
                    rel="noreferrer"
                    onClick={(event) => {
                      event.preventDefault();
                      setError("");
                      if (!source.url) return;
                      void openSourceUrl(source.url).catch((e) =>
                        setError(friendlyError(e)),
                      );
                    }}
                  >
                    <span className="search-source-id" aria-hidden="true">
                      [{source.id}]
                    </span>
                    <span className="search-source-title">{source.title}</span>
                    <svg
                      className="search-source-external"
                      width="15"
                      height="15"
                      viewBox="0 0 24 24"
                      fill="none"
                      stroke="currentColor"
                      strokeWidth="1.8"
                      strokeLinecap="round"
                      strokeLinejoin="round"
                      aria-hidden="true"
                    >
                      <path d="M14 3h7v7M21 3 10 14M10 3H5a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-5" />
                    </svg>
                  </a>
                ) : (
                  <span
                    className="search-source-unlinked"
                    title="原文链接不可用"
                  >
                    <span className="search-source-id" aria-hidden="true">
                      [{source.id}]
                    </span>
                    <span className="search-source-title">{source.title}</span>
                  </span>
                )}
              </li>
            ))}
          </ul>
        </div>
      ) : null}
      {error ? <p role="alert">{error}</p> : null}
    </div>
  );
}
