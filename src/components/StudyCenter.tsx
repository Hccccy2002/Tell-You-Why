import { useState } from "react";
import type { KnowledgeCard, ProviderSpec, TopicPreference } from "../types";
import type { StudyClient, StudyDueItem, StudyDoubtItem } from "../lib/study";
import { StudyHome } from "./StudyHome";
import { StudyPanel } from "./StudyPanel";
import "../study.css";

export type StudyEntry =
  | { kind: "session"; sessionId?: string; sourceId?: string }
  | { kind: "card"; card: KnowledgeCard; expanded: boolean }
  | { kind: "review"; target: StudyDueItem }
  | { kind: "doubt"; target: StudyDoubtItem };

interface Props {
  modelBusy?: boolean;
  refreshToken?: number;
  initialEntry?: StudyEntry;
  topics: TopicPreference[];
  providers: ProviderSpec[];
  onModelSettings: () => void;
  onExit?: () => void;
  onOpenCard: (card: KnowledgeCard, expanded: boolean) => void;
  client?: StudyClient;
}

export function StudyCenter({
  modelBusy,
  refreshToken,
  initialEntry,
  topics,
  providers,
  onModelSettings,
  onExit,
  onOpenCard,
  client,
}: Props) {
  const [entry, setEntry] = useState<StudyEntry | null>(initialEntry ?? null);
  if (entry)
    return (
      <StudyPanel
        modelBusy={modelBusy}
        topics={topics}
        providers={providers}
        client={client}
        sessionId={entry.kind === "session" ? entry.sessionId : undefined}
        focusSourceId={entry.kind === "session" ? entry.sourceId : undefined}
        cardTarget={entry.kind === "card" ? entry.card : undefined}
        cardExpanded={entry.kind === "card" && entry.expanded}
        reviewTarget={entry.kind === "review" ? entry.target : undefined}
        doubtTarget={entry.kind === "doubt" ? entry.target : undefined}
        onExit={() => setEntry(null)}
        onModelSettings={onModelSettings}
        onOpenCard={onOpenCard}
        exitLabel="返回学习首页"
      />
    );
  return (
    <main className="study-page" aria-labelledby="study-center-title">
      {onExit ? (
        <div className="study-topbar">
          <button className="text-button" onClick={onExit}>
            ← 返回知识小窗
          </button>
        </div>
      ) : null}
      <header className="study-heading">
        <span className="eyebrow">学习中心</span>
        <h1 id="study-center-title">接着学一点</h1>
      </header>
      <StudyHome
        refreshToken={refreshToken}
        busy={false}
        client={client}
        onOpen={(sessionId) => setEntry({ kind: "session", sessionId })}
        onReview={(target) => setEntry({ kind: "review", target })}
        onDoubt={(target) => setEntry({ kind: "doubt", target })}
      />
    </main>
  );
}
