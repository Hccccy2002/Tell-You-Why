import { invoke } from "@tauri-apps/api/core";
import { withAutoHideGuard } from "./autoHideGuard";
import type { Evidence, RagTask } from "./rag";
import type { ReviewRun, ReviewTrace } from "./reviewAgent";

export type EvaluationKind = "review" | "top5" | "model";
export interface EvaluationJob {
  id: string;
  kind: EvaluationKind;
  title: string;
  status:
    | "running"
    | "cancelling"
    | "completed"
    | "cancelled"
    | "failed"
    | "interrupted";
  created_at: string;
  finished_at: string | null;
  completed: number;
  total: number;
  message: string;
  error: string | null;
  model: string | null;
}
export interface EvaluationReport {
  title: string;
  dataset: string;
  notes: string[];
  metrics: {
    label: string;
    value: string;
    baseline: string | null;
    detail?: string | null;
  }[];
  human_review?: {
    report_sha256?: string;
    reviewed?: number;
    total?: number;
    drafts?: number;
    error: string | null;
  };
  rows: {
    id: string;
    title: string;
    status: "passed" | "failed" | "unreviewed";
    note: string;
    human_review?: {
      result_sha256: string;
      annotation: HumanReview | null;
    };
    details: {
      checks?: Record<string, boolean>;
      execution?: { run?: ReviewRun; trace?: ReviewTrace };
      before?: { sources: Evidence[] };
      after?: { sources: Evidence[] };
      result?: { draft?: RagTask["result"]; error?: string };
      reference_answer?: string;
      evidence?: Evidence[];
      run?: ReviewRun;
      trace?: ReviewTrace;
    };
  }[];
}
export interface HumanReviewInput {
  reviewer: string;
  correct: boolean | null;
  complete: boolean | null;
  grounded: boolean | null;
  notes: string;
}
export interface HumanReview extends HumanReviewInput {
  id: string;
  result_sha256: string;
  method: "human" | null;
  revision: number;
  updated_at: string;
}
export interface SaveHumanReview {
  id: string;
  case_id: string;
  report_sha256: string;
  expected_revision: number;
  review: HumanReviewInput;
}
export interface EvaluationDocument {
  job: EvaluationJob;
  report: EvaluationReport | null;
}
export const evaluationList = () =>
  invoke<{ jobs: EvaluationJob[]; unavailable_reason: string | null }>(
    "evaluation_list",
  );
export const evaluationRead = (id: string) =>
  invoke<EvaluationDocument>("evaluation_read", { id });
export const evaluationSaveReview = (request: SaveHumanReview) =>
  invoke<EvaluationDocument>("evaluation_save_review", { request });
export const evaluationStart = (request: {
  kind: EvaluationKind;
  provider?: string;
  region?: string;
}) => invoke<EvaluationJob>("evaluation_start", { request });
export const evaluationCancel = (id: string) =>
  invoke<void>("evaluation_cancel", { id });
export const evaluationExport = (id: string) =>
  withAutoHideGuard("evaluation-export", () =>
    invoke<string | null>("evaluation_export", { id }),
  );
