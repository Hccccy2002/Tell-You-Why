import { invoke } from "@tauri-apps/api/core";
import type { RagTask } from "./rag";
import type { Chapter } from "./knowledgeBase";

export type LearningStatus = "new" | "learning" | "review" | "mastered";
export interface LearningState {
  card_id: string;
  status: LearningStatus;
  shown_count: number;
  revealed_count: number;
  revision: number;
}
export interface Presentation {
  id: string;
  card_id: string;
  confirmed: boolean;
  revealed: boolean;
  expires_at: string;
}
export interface LearningSession {
  id: string;
  kb: string;
  chapter: string | null;
  chapter_path: string[];
  filter: string;
  relaxed: boolean;
  history: Presentation[];
  cursor: number | null;
  revision: number;
}
export interface LearningView {
  session: LearningSession | null;
  card: RagTask | null;
  state: LearningState | null;
  summary: Record<LearningStatus | "today" | "covered_units", number>;
  reason: string | null;
}
export interface LearningEvent {
  id: string;
  session_id: string;
  presentation_id: string;
  kind: "shown" | "revealed" | "skipped" | "status" | "undo";
  status: LearningStatus | null;
  expected_revision: number;
  undo_event_id: string | null;
}
export const learningResume = (kb: string) =>
  invoke<LearningView>("learning_resume", { kb });
export const learningStart = (request: {
  kb: string;
  chapter: string | null;
  chapter_path: string[];
  filter: string;
  relaxed: boolean;
}) => invoke<LearningView>("learning_start", { request });
export const learningNext = (s: LearningSession) =>
  invoke<LearningView>("learning_next", {
    sessionId: s.id,
    revision: s.revision,
  });
export const learningPrevious = (s: LearningSession) =>
  invoke<LearningView>("learning_previous", {
    sessionId: s.id,
    revision: s.revision,
  });
export const learningRecord = (event: LearningEvent) =>
  invoke<LearningView>("learning_record_event", { event });
export const learningReset = (kb: string) =>
  invoke<LearningView>("learning_reset", { kb });
export function chapterPath(chapters: Chapter[], id: string): string[] {
  const stack: { depth: number; title: string }[] = [];
  for (const chapter of chapters) {
    while (stack.length && stack[stack.length - 1]!.depth >= chapter.depth)
      stack.pop();
    stack.push(chapter);
    if (chapter.id === id) return stack.map((c) => c.title);
  }
  return [];
}
