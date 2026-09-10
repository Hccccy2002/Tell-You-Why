import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { vi } from "vitest";
import { RandomLearningCard } from "./RandomLearningCard";
import * as api from "../lib/rag";
import type { RagTask } from "../lib/rag";
import type { PdfKnowledgeBase } from "../lib/knowledgeBase";
vi.mock("../lib/rag", () => ({
  ragGenerate: vi.fn(),
  ragPrepareRandom: vi.fn(),
  ragProviders: vi.fn(),
}));
const book: PdfKnowledgeBase = {
  id: "b",
  version: "v1",
  filename: "教材.pdf",
  pages: 10,
  status: "ready",
  chunks: 3,
  coverage: null,
  job: null,
};
const task: RagTask = {
  id: "t",
  kb: "b",
  kind: "card",
  state: "prepared",
  provider: "deepseek",
  region: "default",
  model: "model",
  created_at: "2026-09-10T00:00:00Z",
  usage: [],
  error: null,
  result: null,
  packet: {
    kb: "b",
    version: "v1",
    source_sha256: "s",
    filename: "教材.pdf",
    chapter: "c1",
    query: "随机学习",
    status: "candidates",
    sha256: "hash",
    text_chars: 5,
    evidence: [
      {
        id: "E1",
        block_id: "b1",
        page: 2,
        text: "主素材正文",
        chapter_path: ["第一章"],
      },
    ],
  },
};
beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(api.ragProviders).mockResolvedValue([
    { id: "deepseek", region: "default", model: "model" },
  ]);
  vi.mocked(api.ragPrepareRandom).mockResolvedValue(task);
  vi.mocked(api.ragGenerate).mockResolvedValue({
    ...task,
    state: "completed",
    result: {
      status: "answered",
      question: "问题",
      answer: [],
      explanation: [],
      reason: "",
    },
  });
});
async function preview() {
  const user = userEvent.setup();
  await user.click(screen.getByRole("button", { name: "生成新卡" }));
  await waitFor(() =>
    expect(screen.getByRole("button", { name: "预览原文" })).toBeEnabled(),
  );
  await user.click(screen.getByRole("button", { name: "预览原文" }));
  await screen.findByText("主素材正文");
  return user;
}
it("previews selected scope before any cloud call and refreshes learning only on success", async () => {
  const saved = vi.fn();
  render(<RandomLearningCard book={book} chapter="c1" onSaved={saved} />);
  const user = await preview();
  expect(api.ragPrepareRandom).toHaveBeenCalledWith({
    kb: "b",
    version: "v1",
    chapter: "c1",
    provider: "deepseek",
    region: "default",
  });
  expect(api.ragGenerate).not.toHaveBeenCalled();
  await user.click(screen.getByRole("button", { name: "发送摘录并生成新卡" }));
  await screen.findByText(/新卡已保存为未学习/);
  expect(saved).toHaveBeenCalledTimes(1);
  expect(api.ragGenerate).toHaveBeenCalledTimes(1);
});
it("reports insufficient evidence without retry or changing learning state", async () => {
  vi.mocked(api.ragGenerate).mockResolvedValue({
    ...task,
    state: "completed",
    result: {
      status: "insufficient",
      question: "问题",
      answer: [],
      explanation: [],
      reason: "条件不完整",
    },
  });
  const saved = vi.fn();
  render(<RandomLearningCard book={book} chapter="" onSaved={saved} />);
  const user = await preview();
  await user.click(screen.getByRole("button", { name: "发送摘录并生成新卡" }));
  await screen.findByText("依据不足：条件不完整");
  expect(saved).not.toHaveBeenCalled();
  expect(api.ragGenerate).toHaveBeenCalledTimes(1);
});
it("does not require providers until the generation controls are opened", () => {
  render(<RandomLearningCard book={book} chapter="" onSaved={vi.fn()} />);
  expect(api.ragProviders).not.toHaveBeenCalled();
  expect(api.ragGenerate).not.toHaveBeenCalled();
});
