import type { useStudyHighlights } from "../lib/useStudyHighlights";
import { SearchAnswerView } from "./SearchAnswerView";

export function StudyHighlights({
  notes,
  onOpen,
  disabled = false,
}: {
  notes: ReturnType<typeof useStudyHighlights>;
  onOpen?: (id: string, sourceId: string) => void;
  disabled?: boolean;
}) {
  if (!notes.items.length && !notes.error) return null;
  return (
    <section className="study-highlights" aria-label="学习收获">
      <h3>学习收获</h3>
      <p className="study-home-note">你选择保存的原文 · AI 内容未经事实核验</p>
      {notes.error ? (
        <p role="alert">
          学习收获操作未完成：{notes.error}{" "}
          <button disabled={notes.busy || disabled} onClick={notes.reload}>
            重新读取
          </button>
        </p>
      ) : null}
      {notes.items.map((note) => (
        <article key={note.id}>
          <h4>{note.title}</h4>
          {note.search ? (
            <SearchAnswerView answer={note.search} />
          ) : (
            <p>{note.text}</p>
          )}
          <div className="study-actions">
            {onOpen ? (
              <button
                className="text-button"
                disabled={disabled || notes.busy}
                onClick={() => onOpen(note.session_id, note.source_id)}
              >
                查看原始问答
              </button>
            ) : null}
            <button
              className="text-button"
              disabled={disabled || notes.busy}
              onClick={() => void notes.remove(note.id)}
            >
              移出学习收获
            </button>
          </div>
        </article>
      ))}
    </section>
  );
}
