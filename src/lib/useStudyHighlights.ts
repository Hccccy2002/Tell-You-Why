import { useCallback, useEffect, useRef, useState } from "react";
import { friendlyError } from "./api";
import { studyClient, type StudyClient, type StudyHighlight } from "./study";

type Client = Pick<
  StudyClient,
  "highlights" | "saveHighlight" | "removeHighlight"
>;

export function useStudyHighlights(
  sessionId?: string,
  cardId?: string,
  client: Client = studyClient,
) {
  const scope = sessionId ? `session:${sessionId}` : `card:${cardId ?? ""}`;
  const [result, setResult] = useState<{
    scope: string;
    items: StudyHighlight[];
    error: string | null;
  } | null>(null);
  const [revision, setRevision] = useState(0);
  const [busy, setBusy] = useState(false);
  const busyRef = useRef(false);
  const epoch = useRef(0);
  const reload = useCallback(() => setRevision((n) => n + 1), []);
  useEffect(() => {
    if (!sessionId && !cardId) return;
    let active = true;
    const request = ++epoch.current;
    void client
      .highlights({ sessionId, cardId })
      .then((items) => {
        if (active && epoch.current === request)
          setResult({ scope, items, error: null });
      })
      .catch((e: unknown) => {
        if (active && epoch.current === request)
          setResult({ scope, items: [], error: friendlyError(e) });
      });
    return () => {
      active = false;
    };
  }, [sessionId, cardId, scope, client, revision]);
  const items = result?.scope === scope ? result.items : [];
  async function mutate(action: () => Promise<unknown>) {
    if (busyRef.current) return;
    busyRef.current = true;
    const request = ++epoch.current;
    setBusy(true);
    try {
      await action();
      const next = await client.highlights({ sessionId, cardId });
      if (epoch.current === request)
        setResult({ scope, items: next, error: null });
    } catch (e) {
      if (epoch.current === request)
        setResult({ scope, items, error: friendlyError(e) });
    } finally {
      busyRef.current = false;
      setBusy(false);
    }
  }
  return {
    items,
    busy,
    reload,
    error: result?.scope === scope ? result.error : null,
    saved: (kind: string, sourceId: string) =>
      items.some((n) => n.source_kind === kind && n.source_id === sourceId),
    save: (kind: "step" | "question", sourceId: string) =>
      sessionId
        ? mutate(() => client.saveHighlight(sessionId, kind, sourceId))
        : Promise.resolve(),
    remove: (id: string) => mutate(() => client.removeHighlight(id)),
  };
}
