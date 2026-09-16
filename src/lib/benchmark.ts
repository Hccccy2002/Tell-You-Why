import { invoke } from "@tauri-apps/api/core";
import { withAutoHideGuard } from "./autoHideGuard";

export const splitLabels = {
  development: "开发集",
  regression: "回归集",
  holdout: "留出测试集",
};
export const kindLabels = {
  single: "直接查询",
  multi: "跨段信息",
  contrast: "概念辨析",
  absent: "资料没有答案",
  unsupported_visual: "图表信息不足",
  ocr_error: "OCR 错误",
  false_premise: "错误前提",
};
export const behaviorLabels = {
  answer: "依据原文回答",
  refuse: "说明证据不足并拒答",
  correct_premise: "纠正问题前提后回答",
};
export interface LabelEvidence {
  block_id: string;
  page: number;
  quote: string;
  text_sha256: string;
  eligible_at_annotation: boolean;
}
export interface LabelReview {
  method: string;
  human_reviewer: string | null;
  date: string | null;
  notes?: string;
}
export interface BenchmarkCase {
  id: string;
  question: string;
  split: keyof typeof splitLabels;
  kind: keyof typeof kindLabels;
  expected_behavior: keyof typeof behaviorLabels;
  reference_answer: string;
  acceptance_criteria: string;
  evidence: LabelEvidence[];
  origin: string;
  review: LabelReview;
}
export interface BenchmarkDocument {
  sources?: {
    kb: string;
    knowledge_version: string;
    source_filename: string;
    status: string;
  }[];
  revision: number;
  dataset: {
    kb: string;
    knowledge_version: string;
    source_filename: string;
    source_pages: number;
    cases: BenchmarkCase[];
  };
  releases: {
    id: string;
    version: string;
    source_filename?: string;
    created_at: string;
    cases: Pick<BenchmarkCase, "id" | "split" | "question">[];
  }[];
  uses: {
    job_id: string;
    split: string;
    release: string;
    used_before: number;
  }[];
}
export interface BenchmarkSelection {
  release: string;
  split: keyof typeof splitLabels;
  case_ids: string[];
  acknowledge_holdout: boolean;
}
export interface BenchmarkConfig {
  selection: BenchmarkSelection | null;
  valid: boolean;
}
export const evaluationBenchmark = <T = BenchmarkDocument>(
  request: Record<string, unknown>,
) => invoke<T>("evaluation_benchmark", { request });
export const exportBenchmark = (release: string) =>
  withAutoHideGuard("benchmark-export", () =>
    invoke<string | null>("evaluation_benchmark_export", { release }),
  );
