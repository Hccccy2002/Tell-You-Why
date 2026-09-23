import { invoke } from "@tauri-apps/api/core";
import { withAutoHideGuard } from "./autoHideGuard";

export interface RagProvider {
  id: string;
  region: string;
  model: string;
}
export interface Evidence {
  id: string;
  text: string;
  block_id: string;
  page: number;
  chapter_path: string[];
}
export interface EvidencePacket {
  kb: string;
  version: string;
  filename: string;
  source_sha256: string;
  query: string;
  chapter: string | null;
  chapter_path?: string[];
  evidence: Evidence[];
  text_chars: number;
  status: string;
  sha256: string;
}
export interface RagClaim {
  text: string;
  citations: { evidence_id: string; quote: string }[];
}
export interface RagTask {
  id: string;
  kb: string;
  kind: "ask" | "card";
  state: "prepared" | "running" | "completed" | "failed";
  provider: string;
  region: string;
  model: string;
  created_at: string;
  packet: EvidencePacket;
  error: string | null;
  duplicate_card?: boolean;
  card_id?: string;
  result: {
    status: "answered" | "insufficient";
    question: string;
    answer: RagClaim[];
    explanation: RagClaim[];
    reason: string;
    generation_mode?: "llm";
  } | null;
  usage: {
    call: number;
    status: string | number;
    usage: { total_tokens?: number } | null;
  }[];
}
export const ragProviders = () => invoke<RagProvider[]>("rag_providers");
export interface RelatedSourcesRequest {
  kb: string;
  version: string;
  chapter: string | null;
  query: string;
  source_sha256?: string;
}
export const ragRelatedSources = (request: RelatedSourcesRequest) =>
  invoke<EvidencePacket>("rag_related_sources", { request });
export const ragPrepareRandom = (request: {
  kb: string;
  version: string;
  chapter: string | null;
  provider: string;
  region: string;
}) => invoke<RagTask>("rag_prepare_random", { request });
export const ragGenerate = (taskId: string) =>
  withAutoHideGuard("textbook-generation", () =>
    invoke<RagTask>("rag_generate", { taskId }),
  );
