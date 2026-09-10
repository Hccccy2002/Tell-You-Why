import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { isDesktop } from "./api";

export interface ImportJob {
  id: string;
  status: "running" | "cancelling" | "paused" | "failed" | "completed";
  stage: string;
  total: number;
  completed: number;
  counts: Record<string, number>;
  error: string | null;
  page_errors: { page: number; error: string }[];
}

export interface PdfKnowledgeBase {
  id: string;
  filename: string;
  pages: number;
  status: "ready" | "partial_ready" | "not_ready";
  version: string | null;
  chunks: number;
  coverage: { eligible_fraction_of_recognized_body: number } | null;
  job: ImportJob | null;
}

export interface Catalog {
  items: PdfKnowledgeBase[];
  errors: { id: string; error: string }[];
  models_ready: boolean;
  import_running: boolean;
  launch_error?: string | null;
}

export interface PdfSelection {
  path: string;
  filename: string;
  pages: number;
  bytes: number;
}

export interface Chapter {
  id: string;
  title: string;
  depth: number;
  kind: string;
  start_page: number | null;
}

export interface Passage {
  id?: string;
  chunk_id?: string;
  text: string;
  chapter_path: string[];
  locations: { page: number; printed_label?: string | null }[];
}

export interface PagePreview {
  page: number;
  printed_label: string | null;
  image: string;
}

export function kbRead<T>(request: Record<string, unknown>): Promise<T> {
  if (!isDesktop())
    return Promise.reject(new Error("请在桌面 APP 中使用 PDF 知识库。"));
  return invoke<T>("kb_read", { request });
}

export async function choosePdf(): Promise<PdfSelection | null> {
  if (!isDesktop()) throw new Error("请在桌面 APP 中导入 PDF。");
  const selected = await open({
    multiple: false,
    directory: false,
    title: "选择要建立知识库的 PDF",
    filters: [{ name: "PDF 文档", extensions: ["pdf"] }],
  });
  if (!selected || Array.isArray(selected)) return null;
  const info = await kbRead<Omit<PdfSelection, "path">>({
    op: "inspect",
    pdf: selected,
  });
  return { ...info, path: selected };
}

export const importPdf = (pdf: string, firstPage: number) =>
  invoke<{ kb: string; job: string; reused: boolean }>("kb_import", {
    pdf,
    firstPage,
  });
export const resumePdf = (kb: string, job: string) =>
  invoke<void>("kb_resume", { kb, job });
export const pausePdf = (kb: string, job: string) =>
  invoke<void>("kb_pause", { kb, job });
