import { act, render, screen, waitFor } from "@testing-library/react";
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
it("generates and saves with one click using the selected chapter without opening a preview", async () => {
  const saved = vi.fn();
  render(<RandomLearningCard book={book} chapter="c1" onSaved={saved} />);
  const user = userEvent.setup();
  await user.click(screen.getByRole("button", { name: "生成新卡" }));
  await screen.findByText(/新卡已保存为未学习/);
  expect(api.ragPrepareRandom).toHaveBeenCalledWith({
    kb: "b",
    version: "v1",
    chapter: "c1",
    provider: "deepseek",
    region: "default",
  });
  expect(api.ragGenerate).toHaveBeenCalledExactlyOnceWith(task.id);
  expect(screen.queryByText("主素材正文")).not.toBeInTheDocument();
  expect(screen.queryByText("本次教材摘录")).not.toBeInTheDocument();
  expect(screen.queryByRole("combobox")).not.toBeInTheDocument();
  expect(saved).toHaveBeenCalledTimes(1);
  expect(api.ragGenerate).toHaveBeenCalledTimes(1);
});
it("saves the LLM fallback without opening source content", async () => {
  vi.mocked(api.ragGenerate).mockResolvedValue({
    ...task,
    state: "completed",
    result: {
      status: "answered",
      question: "问题",
      answer: [{ text: "模型提供的通俗讲解", citations: [] }],
      explanation: [],
      reason: "",
      generation_mode: "llm",
    },
  });
  const saved = vi.fn();
  render(<RandomLearningCard book={book} chapter="" onSaved={saved} />);
  const user = userEvent.setup();
  await user.click(screen.getByRole("button", { name: "生成新卡" }));
  await screen.findByText(/新卡已保存为未学习/);
  expect(saved).toHaveBeenCalledTimes(1);
  expect(screen.queryByText("主素材正文")).not.toBeInTheDocument();
  expect(api.ragGenerate).toHaveBeenCalledTimes(1);
});
it("does not load providers or call the model before the user clicks generate", () => {
  render(<RandomLearningCard book={book} chapter="" onSaved={vi.fn()} />);
  expect(api.ragProviders).not.toHaveBeenCalled();
  expect(api.ragGenerate).not.toHaveBeenCalled();
});

it("keeps one generation in flight and releases the busy state after failure", async () => {
  let fail!: (error: Error) => void;
  vi.mocked(api.ragGenerate).mockImplementationOnce(
    () =>
      new Promise((_, reject) => {
        fail = reject;
      }),
  );
  const saved = vi.fn();
  const busy = vi.fn();
  render(
    <RandomLearningCard
      book={book}
      chapter="c1"
      onSaved={saved}
      onBusyChange={busy}
    />,
  );
  const user = userEvent.setup();
  await user.dblClick(screen.getByRole("button", { name: "生成新卡" }));
  expect(screen.getByRole("button", { name: "生成中…" })).toBeDisabled();
  expect(api.ragGenerate).toHaveBeenCalledTimes(1);
  expect(busy).toHaveBeenLastCalledWith(true);
  await act(async () => {
    fail(new Error("模型请求失败"));
    await Promise.resolve();
  });
  expect(await screen.findByRole("alert")).toHaveTextContent("模型请求失败");
  expect(saved).not.toHaveBeenCalled();
  expect(busy).toHaveBeenLastCalledWith(false);
  expect(screen.getByRole("button", { name: "生成新卡" })).toBeEnabled();
  await user.click(screen.getByRole("button", { name: "生成新卡" }));
  await screen.findByText(/新卡已保存为未学习/);
  expect(saved).toHaveBeenCalledTimes(1);
  expect(api.ragGenerate).toHaveBeenCalledTimes(2);
});

it("reports missing models without preparing evidence or starting generation", async () => {
  vi.mocked(api.ragProviders).mockResolvedValue([]);
  render(<RandomLearningCard book={book} chapter="" onSaved={vi.fn()} />);
  await userEvent
    .setup()
    .click(screen.getByRole("button", { name: "生成新卡" }));
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "请先在模型设置中配置并测试模型通道",
  );
  expect(api.ragPrepareRandom).not.toHaveBeenCalled();
  expect(api.ragGenerate).not.toHaveBeenCalled();
});

it("still generates a card when no usable evidence was selected", async () => {
  vi.mocked(api.ragPrepareRandom).mockResolvedValue({
    ...task,
    packet: { ...task.packet, evidence: [] },
  });
  const saved = vi.fn();
  render(<RandomLearningCard book={book} chapter="c1" onSaved={saved} />);
  await userEvent
    .setup()
    .click(screen.getByRole("button", { name: "生成新卡" }));
  await screen.findByText(/新卡已保存为未学习/);
  expect(api.ragGenerate).toHaveBeenCalledExactlyOnceWith(task.id);
  expect(saved).toHaveBeenCalledTimes(1);
});

it("reports an unavailable model without pretending a card was saved", async () => {
  vi.mocked(api.ragGenerate).mockResolvedValue({
    ...task,
    state: "failed",
    error: "模型服务暂时不可用",
  });
  const saved = vi.fn();
  render(<RandomLearningCard book={book} chapter="" onSaved={saved} />);
  await userEvent
    .setup()
    .click(screen.getByRole("button", { name: "生成新卡" }));
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "模型服务暂时不可用",
  );
  expect(saved).not.toHaveBeenCalled();
  expect(api.ragGenerate).toHaveBeenCalledTimes(1);
});

it("does not start a model call if the panel was unmounted during preparation", async () => {
  let complete!: (task: RagTask) => void;
  vi.mocked(api.ragPrepareRandom).mockImplementation(
    () =>
      new Promise((resolve) => {
        complete = resolve;
      }),
  );
  const mounted = render(
    <RandomLearningCard book={book} chapter="" onSaved={vi.fn()} />,
  );
  await userEvent
    .setup()
    .click(screen.getByRole("button", { name: "生成新卡" }));
  await waitFor(() => expect(api.ragPrepareRandom).toHaveBeenCalledTimes(1));
  mounted.unmount();
  await act(async () => {
    complete(task);
    await Promise.resolve();
  });
  expect(api.ragGenerate).not.toHaveBeenCalled();
});
