import { useEffect, useState } from "react";
import { friendlyError } from "../lib/api";
import { StudyHighlights } from "./StudyHighlights";
import { useStudyHighlights } from "../lib/useStudyHighlights";
import {
  studyClient,
  type StudyCardSession,
  type StudyClient,
} from "../lib/study";

export function CardStudyActivity({
  cardId,
  onOpen,
  disabled,
  client = studyClient,
}: {
  cardId: string;
  onOpen: (id: string, sourceId?: string) => void;
  disabled: boolean;
  client?: Pick<StudyClient, "cardSessions">;
}) {
  const notes = useStudyHighlights(undefined, cardId);
  const [result, setResult] = useState<{
    cardId: string;
    sessions: StudyCardSession[];
    error: string | null;
  } | null>(null);
  const sessions = result?.cardId === cardId ? result.sessions : [];
  const error = result?.cardId === cardId ? result.error : null;
  const [reload, setReload] = useState(0);
  useEffect(() => {
    let active = true;
    void client
      .cardSessions(cardId)
      .then((items) => {
        if (active) setResult({ cardId, sessions: items, error: null });
      })
      .catch((e: unknown) => {
        if (active)
          setResult({ cardId, sessions: [], error: friendlyError(e) });
      });
    return () => {
      active = false;
    };
  }, [cardId, client, reload]);
  if (error)
    return (
      <p role="status">
        学习记录暂时无法读取：{error}{" "}
        <button disabled={disabled} onClick={() => setReload((n) => n + 1)}>
          重新读取学习记录
        </button>
      </p>
    );
  const latest = sessions[0];
  return (
    <>
      {latest ? (
        <section className="study-card-activity" aria-label="这张卡的学习记录">
          <h3>上次围绕这张卡的学习</h3>
          <button
            className="secondary-button"
            disabled={disabled}
            onClick={() => onOpen(latest.id)}
          >
            {latest.state === "completed" ? "回看上次学习" : "继续这张卡的学习"}{" "}
            · {latest.step_count} 步
          </button>
          {sessions.length > 1 ? (
            <details>
              <summary>更早的学习</summary>
              {sessions.slice(1).map((item) => (
                <p key={item.id}>
                  <button
                    className="text-button"
                    disabled={disabled}
                    onClick={() => onOpen(item.id)}
                  >
                    {new Date(item.created_at).toLocaleDateString("zh-CN")} ·{" "}
                    {item.step_count} 步
                  </button>
                </p>
              ))}
            </details>
          ) : null}
        </section>
      ) : null}
      <StudyHighlights notes={notes} disabled={disabled} onOpen={onOpen} />
    </>
  );
}
