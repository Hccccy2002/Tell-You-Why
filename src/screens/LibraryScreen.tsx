import { useEffect, useState } from "react";
import {
  clearData,
  deleteLibraryCard,
  friendlyError,
  listLibrary,
} from "../lib/api";
import type { KnowledgeCard, LibraryItem, TopicPreference } from "../types";
import { ConfirmationDialog } from "../components/ConfirmationDialog";

type DeleteConfirmation =
  | { kind: "card"; cardId: string; question: string }
  | { kind: "history" }
  | null;

interface Props {
  topics: TopicPreference[];
  refreshToken: number;
  onOpenCard: (card: KnowledgeCard) => void;
  onCardDeleted: (cardId: string) => void;
  onHistoryCleared: () => void;
}

export function LibraryScreen({
  topics,
  refreshToken,
  onOpenCard,
  onCardDeleted,
  onHistoryCleared,
}: Props) {
  const [mode, setMode] = useState<"favorites" | "history">("favorites");
  const [topicId, setTopicId] = useState("");
  const [sort, setSort] = useState<"newest" | "oldest">("newest");
  const [items, setItems] = useState<LibraryItem[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [deletingId, setDeletingId] = useState<string | null>(null);
  const [clearingHistory, setClearingHistory] = useState(false);
  const [confirmation, setConfirmation] = useState<DeleteConfirmation>(null);

  useEffect(() => {
    let active = true;
    void listLibrary(mode, topicId || null, sort)
      .then((result) => {
        if (active) setItems(result);
      })
      .catch((reason: unknown) => {
        if (active) setError(friendlyError(reason));
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => {
      active = false;
    };
  }, [mode, topicId, sort, refreshToken]);

  async function removeCard(cardId: string) {
    setDeletingId(cardId);
    setError(null);
    try {
      await deleteLibraryCard(cardId);
      setItems((current) => current.filter((item) => item.card.id !== cardId));
      onCardDeleted(cardId);
      setConfirmation(null);
    } catch (reason) {
      setError(friendlyError(reason));
    } finally {
      setDeletingId(null);
    }
  }

  async function clearHistory() {
    setClearingHistory(true);
    setError(null);
    try {
      await clearData("history");
      setItems([]);
      onHistoryCleared();
      setConfirmation(null);
    } catch (reason) {
      setError(friendlyError(reason));
    } finally {
      setClearingHistory(false);
    }
  }

  const deleting = deletingId !== null || clearingHistory;

  return (
    <main className="page-view library-view">
      <div className="page-heading">
        <span className="eyebrow">留住值得再看的内容</span>
        <h1>收藏与历史</h1>
      </div>
      <div className="segmented-control" aria-label="内容列表类型">
        <button
          className={mode === "favorites" ? "active" : ""}
          onClick={() => {
            setConfirmation(null);
            setMode("favorites");
          }}
        >
          收藏
        </button>
        <button
          className={mode === "history" ? "active" : ""}
          onClick={() => {
            setConfirmation(null);
            setMode("history");
          }}
        >
          最近浏览
        </button>
      </div>
      <div className="filter-row">
        <label>
          <span className="sr-only">按领域筛选</span>
          <select
            value={topicId}
            onChange={(event) => setTopicId(event.target.value)}
          >
            <option value="">全部领域</option>
            {topics
              .filter((topic) => topic.enabled)
              .map((topic) => (
                <option key={topic.id} value={topic.id}>
                  {topic.label}
                </option>
              ))}
          </select>
        </label>
        <label>
          <span className="sr-only">时间排序</span>
          <select
            value={sort}
            onChange={(event) =>
              setSort(event.target.value as "newest" | "oldest")
            }
          >
            <option value="newest">最近优先</option>
            <option value="oldest">最早优先</option>
          </select>
        </label>
      </div>
      {mode === "history" ? (
        <div className="library-history-actions">
          <span>{items.length} 条浏览记录 · 最多保留最近 500 条</span>
          <button
            className="library-clear-all"
            disabled={loading || items.length === 0 || deleting}
            onClick={() => {
              setError(null);
              setConfirmation({ kind: "history" });
            }}
          >
            清除阅读记录
          </button>
        </div>
      ) : null}
      {loading ? (
        <div className="list-status" aria-live="polite">
          正在读取本地记录…
        </div>
      ) : null}
      {error && !confirmation ? (
        <div className="inline-error" role="alert">
          {error}
        </div>
      ) : null}
      {!loading && !error && items.length === 0 ? (
        <div className="empty-state">
          <span aria-hidden="true">{mode === "favorites" ? "☆" : "◷"}</span>
          <h2>{mode === "favorites" ? "还没有收藏" : "还没有浏览记录"}</h2>
          <p>
            {mode === "favorites"
              ? "遇到想再看的问题时，点一下收藏。"
              : "读过的知识卡会出现在这里。"}
          </p>
        </div>
      ) : null}
      <div className="library-list">
        {items.map(({ card, viewedAt }) => (
          <div key={card.id + "-" + viewedAt} className="library-item">
            <button className="library-open" onClick={() => onOpenCard(card)}>
              <span className="library-meta">
                <span>{card.topicLabel}</span>
                <time dateTime={viewedAt}>
                  {new Intl.DateTimeFormat("zh-CN", {
                    month: "numeric",
                    day: "numeric",
                  }).format(new Date(viewedAt))}
                </time>
              </span>
              <strong>{card.question}</strong>
              <span className="library-answer">{card.shortAnswer}</span>
            </button>
            {mode === "history" ? (
              <button
                className="library-delete"
                aria-label={"删除：" + card.question}
                disabled={deleting}
                onClick={() => {
                  setError(null);
                  setConfirmation({
                    kind: "card",
                    cardId: card.id,
                    question: card.question,
                  });
                }}
              >
                {deletingId === card.id ? "删除中…" : "删除"}
              </button>
            ) : null}
          </div>
        ))}
      </div>
      {confirmation ? (
        <ConfirmationDialog
          id="library-delete-confirmation"
          eyebrow="删除确认"
          title={
            confirmation.kind === "card"
              ? "删除这条知识卡？"
              : "清除所有阅读记录？"
          }
          confirmLabel={
            confirmation.kind === "card" ? "删除知识卡" : "清除阅读记录"
          }
          busyLabel={
            confirmation.kind === "card"
              ? "正在删除知识卡…"
              : "正在清除阅读记录…"
          }
          busy={deleting}
          onCancel={() => setConfirmation(null)}
          onConfirm={() => {
            if (confirmation.kind === "card") {
              void removeCard(confirmation.cardId);
            } else {
              void clearHistory();
            }
          }}
        >
          {confirmation.kind === "card" ? (
            <>
              <p className="confirmation-target">“{confirmation.question}”</p>
              <p>删除后，这条知识点将无法在应用中恢复。</p>
            </>
          ) : (
            <p>最近浏览将被清空，收藏和知识卡内容会保留。</p>
          )}
          {error ? (
            <p className="inline-error" role="alert">
              {error}
            </p>
          ) : null}
        </ConfirmationDialog>
      ) : null}
    </main>
  );
}
