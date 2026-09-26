import { invoke } from "@tauri-apps/api/core";
import { withAutoHideGuard } from "./autoHideGuard";
import type { Evidence } from "./rag";

export interface CompletionReport {
  version: string;
  outcome: "awaiting_answer" | "completed" | "needs_repair" | "rejected";
  checks: {
    code: string;
    label: string;
    status: "passed" | "pending" | "failed";
  }[];
  verified_submissions: number;
  content_quality: "not_assessed";
}

export interface ReviewQuestion {
  id: string;
  topic: string;
  question: string;
  options: string[];
  selected_index: number | null;
  correct: boolean | null;
  correct_index: number | null;
  explanation: string | null;
  source_ids: string[];
}
export interface ReviewRun {
  id: string;
  scope: {
    kb: string;
    version: string;
    filename: string;
    chapter: string | null;
    chapter_path: string[];
  };
  goal: string;
  state:
    | "ready"
    | "running"
    | "waiting_answer"
    | "completed"
    | "paused"
    | "failed"
    | "stopped";
  provider: string;
  model: string;
  created_at: string;
  output: string;
  error: string | null;
  cancel_requested: boolean;
  model_calls: number;
  tool_calls: number;
  can_resume?: boolean;
  completion?: CompletionReport | null;
  contract?: {
    min_questions: number;
    max_questions: number;
    require_sources: boolean;
    max_repairs: number;
  };
  stop_reason?: { code: string; message: string; resumable: boolean } | null;
  questions: ReviewQuestion[];
  sources: Evidence[];
  mcp_servers?: { id: string; name: string; tool_count: number }[];
}
export const reviewStart = (request: {
  kb: string;
  version: string;
  chapter: string | null;
  goal: string;
  provider: string;
  region: string;
  due_only?: boolean;
  question_count?: number | null;
  require_sources?: boolean;
  mcp_server_ids?: string[];
}) => invoke<ReviewRun>("review_start", { request });
export const reviewContinue = (id: string) =>
  withAutoHideGuard("review-agent", () =>
    invoke<ReviewRun>("review_continue", { id }),
  );
export const reviewLatest = (kb: string) =>
  invoke<ReviewRun | null>("review_latest", { kb });
export const reviewRead = (id: string) =>
  invoke<ReviewRun>("review_read", { id });
export const reviewAnswer = (
  id: string,
  questionId: string,
  selected: number,
) => invoke<ReviewRun>("review_answer", { id, questionId, selected });
export const reviewCancel = (id: string) =>
  invoke<void>("review_cancel", { id });

export interface ReviewSummary {
  id: string;
  kb: string;
  version: string;
  goal: string;
  state: ReviewRun["state"];
  created_at: string;
  provider: string;
  model: string;
  question_count: number;
}
export interface ReviewTrace {
  schema_version: string;
  run: ReviewSummary;
  prompt_version: string;
  model_requests: number;
  tool_calls: number;
  reported_total_tokens: number | null;
  usage_reported_requests: number;
  harness?: {
    version: string;
    policy: {
      max_model_calls: number;
      max_tool_calls: number;
      max_token_charge: number;
      max_active_ms: number;
    };
    charged_tokens: number;
    charged_active_ms: number;
    stop_reason: ReviewRun["stop_reason"];
    completion: CompletionReport | null;
    completion_repairs: number;
    context: {
      input_units: number;
      full_input_units: number;
      max_input_units: number;
      retained_history_messages: number;
      omitted_history_messages: number;
      compacted: boolean;
      estimate_method: string;
    } | null;
  };
  events: {
    sequence: number;
    kind: "model" | "tool" | "state" | "user" | "policy" | "validation";
    name: string;
    status: "started" | "succeeded" | "failed" | "interrupted";
    started_at: string;
    finished_at: string | null;
    duration_ms: number | null;
    details: Record<string, unknown>;
    usage: {
      prompt_tokens?: number;
      completion_tokens?: number;
      total_tokens?: number;
    } | null;
  }[];
}
export const reviewTrace = (id: string) =>
  invoke<ReviewTrace>("review_trace", { id });
export const reviewHistory = (kb: string) =>
  invoke<ReviewSummary[]>("review_history", { kb });
export interface ReviewMemoryOverview {
  total: number;
  due_count: number;
  weak_count: number;
  as_of: string;
  note: string;
  items: {
    id: string;
    topic: string;
    question: string;
    chapter_path: string[];
    attempts: number;
    correct_count: number;
    lapses: number;
    streak: number;
    last_correct: boolean;
    last_reviewed_at: string;
    due_at: string;
    is_due: boolean;
  }[];
}
export const reviewMemory = (
  kb: string,
  version: string,
  chapter: string | null,
) => invoke<ReviewMemoryOverview>("review_memory", { kb, version, chapter });
export const reviewExport = (id: string) =>
  withAutoHideGuard("review-export", () =>
    invoke<string | null>("review_export", { id }),
  );
