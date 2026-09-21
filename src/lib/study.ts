import { invoke } from "@tauri-apps/api/core";
import { withAutoHideGuard } from "./autoHideGuard";
import type { KnowledgeCard, SearchAnswer } from "../types";

export type StudyFeedback =
  | "continue"
  | "understood"
  | "confused"
  | "easy"
  | "example"
  | "skip"
  | "answer";
export interface StudyGoalPlan {
  objectives: {
    id: string;
    title: string;
    criterion: string;
    taught: boolean;
    self_reported_understood: boolean;
    needs_help: boolean;
    verified: boolean;
    correct: boolean | null;
    lesson_step_ids: string[];
  }[];
  checkin_prompt: string;
  checkin_reply: string | null;
  remediation_used: boolean;
  verification_status:
    "not_offered" | "awaiting_answer" | "submitted" | "skipped";
  finished: boolean;
}
export interface StudyStep {
  id: string;
  kind: "concept" | "prerequisite" | "example" | "deeper" | "quiz";
  title: string;
  text: string;
  reason: string;
  card_id: string | null;
  feedback: StudyFeedback | null;
  quiz: {
    options: string[];
    selected: number | null;
    correct_index: number | null;
    correct: boolean | null;
    explanation: string | null;
  } | null;
}
export interface StudySession {
  goal_mode?: boolean;
  goal_plan?: StudyGoalPlan | null;
  can_finish_goal?: boolean;
  can_reopen_goal?: boolean;
  id: string;
  goal: string;
  topic: string;
  provider: string;
  model: string;
  state: "ready" | "running" | "waiting" | "paused" | "failed" | "completed";
  revision: number;
  created_at: string;
  steps: StudyStep[];
  error: string | null;
  next_topic: string | null;
  can_resume: boolean;
  review_target: { concept_key: string; title: string } | null;
  source_card: { card_id: string; title: string } | null;
  source_expanded?: boolean;
  doubt_target?: { id: string; question: string; reason?: string } | null;
  questions: StudyQuestion[];
  can_ask: boolean;
  question_limit: number;
  summary: { topics: string[]; answered: number; correct: number };
}
export interface StudyQuestion {
  search?: { stage: string } | null;
  id: string;
  step_id: string;
  question: string;
  created_at: string;
  feedback?: "understood" | "unresolved" | null;
  doubt_id?: string | null;
  reply_to_question_id?: string | null;
  clarification_replies?: { prompt: string; reply: string }[];
  answer: {
    search?: SearchAnswer | null;
    kind: "explanation" | "comparison" | "example" | "clarification";
    text: string;
    card_id: string | null;
  } | null;
}
export interface StudyClient {
  startGoal: (goal: string) => Promise<StudySession>;
  goalCheckin: (id: string, reply: string) => Promise<StudySession>;
  reopenGoal: (id: string) => Promise<StudySession>;
  questionFeedback: (
    id: string,
    questionId: string,
    feedback: "understood" | "unresolved",
  ) => Promise<StudySession>;
  startDoubt: (doubtId: string) => Promise<StudySession>;
  saveHighlight: (
    id: string,
    kind: "step" | "question",
    sourceId: string,
  ) => Promise<string>;
  highlights: (filter: {
    sessionId?: string;
    cardId?: string;
  }) => Promise<StudyHighlight[]>;
  removeHighlight: (id: string) => Promise<void>;
  cardSessions: (cardId: string) => Promise<StudyCardSession[]>;
  sourceCard: (id: string) => Promise<KnowledgeCard | null>;
  home: () => Promise<StudyHomeData>;
  history: () => Promise<StudySession[]>;
  reset: () => Promise<void>;
  latest: () => Promise<StudySession | null>;
  start: (goal: string, topic: string) => Promise<StudySession>;
  startReview: (conceptKey: string) => Promise<StudySession>;
  startCard: (cardId: string, expanded?: boolean) => Promise<StudySession>;
  ask: (
    id: string,
    stepId: string,
    question: string,
    requestId: string,
    replyToQuestionId?: string,
    forceSearch?: boolean,
  ) => Promise<StudySession>;
  read: (id: string) => Promise<StudySession>;
  continue: (id: string) => Promise<StudySession>;
  feedback: (
    id: string,
    stepId: string,
    feedback: StudyFeedback,
    selected?: number,
  ) => Promise<StudySession>;
  pause: (id: string, finish: boolean) => Promise<StudySession>;
}
export interface StudyHomeData {
  goals?: { id: string; title: string }[];
  doubts?: StudyDoubtItem[];
  due: StudyDueItem[];
  due_count: number;
  practice_count: number;
  next_due_at: number | null;
  personalization_enabled: boolean;
  active: {
    id: string;
    goal: string;
    topic: string;
    state: StudySession["state"];
    step_count: number;
    last_title: string | null;
    updated_at: string;
  } | null;
}
export interface StudyDoubtItem {
  id: string;
  question: string;
  topic: string;
  session_id: string;
  reason?: string;
}
export interface StudyDueItem {
  concept_key: string;
  topic: string;
  title: string;
  attempts: number;
  last_correct: boolean;
  due_at: number;
}
export interface StudyCardSession {
  id: string;
  goal: string;
  state: StudySession["state"];
  created_at: string;
  step_count: number;
}
export interface StudyHighlight {
  search?: SearchAnswer | null;
  id: string;
  session_id: string;
  source_kind: "step" | "question";
  source_id: string;
  title: string;
  text: string;
  created_at: string;
}
export const studyClient: StudyClient = {
  startGoal: (goal) => invoke("study_start_goal", { goal }),
  goalCheckin: (id, reply) => invoke("study_goal_checkin", { id, reply }),
  reopenGoal: (id) => invoke("study_reopen_goal", { id }),
  questionFeedback: (id, questionId, feedback) =>
    invoke("study_question_feedback", { id, questionId, feedback }),
  startDoubt: (doubtId) => invoke("study_start_doubt", { doubtId }),
  saveHighlight: (id, kind, sourceId) =>
    invoke("study_save_highlight", { id, kind, sourceId }),
  highlights: ({ sessionId, cardId }) =>
    typeof window !== "undefined" && window.__TAURI_INTERNALS__ == null
      ? Promise.resolve([])
      : invoke("study_highlights", {
          sessionId: sessionId ?? null,
          cardId: cardId ?? null,
        }),
  removeHighlight: (id) => invoke("study_remove_highlight", { id }),
  cardSessions: (cardId) =>
    typeof window !== "undefined" && window.__TAURI_INTERNALS__ == null
      ? Promise.resolve([])
      : invoke("study_card_sessions", { cardId }),
  sourceCard: (id) => invoke("study_source_card", { id }),
  home: () =>
    typeof window !== "undefined" && window.__TAURI_INTERNALS__ == null
      ? Promise.resolve({
          active: null,
          due: [],
          due_count: 0,
          practice_count: 0,
          next_due_at: null,
          personalization_enabled: true,
        })
      : invoke("study_home"),
  history: () => invoke("study_history"),
  reset: () => invoke("study_reset"),
  latest: () =>
    typeof window !== "undefined" && window.__TAURI_INTERNALS__ == null
      ? Promise.resolve(null)
      : invoke("study_latest"),
  start: (goal, topic) => invoke("study_start", { goal, topic }),
  startReview: (conceptKey) => invoke("study_start_review", { conceptKey }),
  startCard: (cardId, expanded = false) =>
    invoke("study_start_card_view", { cardId, expanded }),
  ask: (id, stepId, question, requestId, replyToQuestionId, forceSearch) =>
    invoke("study_ask", {
      id,
      stepId,
      question,
      requestId,
      replyToQuestionId: replyToQuestionId ?? null,
      forceSearch: forceSearch ?? false,
    }),
  read: (id) => invoke("study_read", { id }),
  continue: (id) =>
    withAutoHideGuard("study-companion", () =>
      invoke("study_continue", { id }),
    ),
  feedback: (id, stepId, feedback, selected) =>
    invoke("study_feedback", {
      id,
      stepId,
      feedback,
      selected: selected ?? null,
    }),
  pause: (id, finish) => invoke("study_pause", { id, finish }),
};
