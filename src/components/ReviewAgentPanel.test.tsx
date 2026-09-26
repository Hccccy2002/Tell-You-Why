import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { vi } from "vitest";
import { ReviewAgentPanel } from "./ReviewAgentPanel";
import * as api from "../lib/reviewAgent";
import { ragProviders, ragRelatedSources } from "../lib/rag";
import type { ReviewRun } from "../lib/reviewAgent";
import type { PdfKnowledgeBase } from "../lib/knowledgeBase";
import { mcpListServers } from "../lib/mcp";

vi.mock("../lib/reviewAgent", () => ({
  reviewStart: vi.fn(),
  reviewContinue: vi.fn(),
  reviewLatest: vi.fn(),
  reviewRead: vi.fn(),
  reviewAnswer: vi.fn(),
  reviewCancel: vi.fn(),
  reviewHistory: vi.fn(),
  reviewMemory: vi.fn(),
  reviewTrace: vi.fn(),
  reviewExport: vi.fn(),
}));
vi.mock("../lib/rag", () => ({
  ragProviders: vi.fn(),
  ragRelatedSources: vi.fn(),
}));
vi.mock("../lib/mcp", async (importOriginal) => {
  const actual = await importOriginal();
  if (!actual || typeof actual !== "object") {
    throw new Error("Failed to load the real MCP module for this test");
  }
  return { ...actual, mcpListServers: vi.fn() };
});
const book: PdfKnowledgeBase = {
  id: "book",
  version: "v1",
  filename: "教材.pdf",
  pages: 10,
  status: "ready",
  chunks: 10,
  coverage: null,
  job: null,
};
const waiting: ReviewRun = {
  id: "r1",
  scope: {
    kb: "book",
    version: "v1",
    filename: "教材.pdf",
    chapter: "c1",
    chapter_path: ["存储器"],
  },
  goal: "带我复习",
  state: "waiting_answer",
  provider: "deepseek",
  model: "test",
  created_at: "2026-09-12T00:00:00Z",
  output: "先思考，再选择。",
  error: null,
  cancel_requested: false,
  model_calls: 4,
  tool_calls: 4,
  questions: [
    {
      id: "q1",
      topic: "存储器",
      question: "存储器保存什么？",
      options: ["程序和数据", "只有图片"],
      correct_index: null,
      explanation: null,
      selected_index: null,
      correct: null,
      source_ids: ["S1"],
    },
  ],
  sources: [
    {
      id: "S1",
      block_id: "b1",
      page: 3,
      chapter_path: ["存储器"],
      text: "存储器用于存放程序和数据。",
    },
  ],
};
const props = () => ({
  book,
  chapters: [
    { id: "c1", title: "存储器", start_page: 1, depth: 0, kind: "chapter" },
  ],
  active: true,
  onPage: vi.fn(),
});

it("shows a terminal stop without offering an ineffective resume", async () => {
  vi.mocked(api.reviewLatest).mockResolvedValue({
    ...waiting,
    state: "stopped",
    can_resume: false,
    error: "本次复习剩余用量预算不足，请开始新的复习",
    stop_reason: {
      code: "token_limit",
      message: "用量预算不足",
      resumable: false,
    },
  });
  render(<ReviewAgentPanel {...props()} />);
  expect(await screen.findByText("本次复习已停止")).toBeInTheDocument();
  expect(
    screen.queryByRole("button", { name: "继续复习" }),
  ).not.toBeInTheDocument();
  expect(screen.getByRole("button", { name: "开始新的复习" })).toBeEnabled();
  expect(api.reviewContinue).not.toHaveBeenCalled();
});
const dueMemory: api.ReviewMemoryOverview = {
  total: 1,
  due_count: 1,
  weak_count: 1,
  as_of: "2026-09-12T00:00:00Z",
  note: "按实际答题安排。",
  items: [
    {
      id: "m1",
      topic: "Cache 写回",
      question: "何时写回？",
      chapter_path: ["存储器"],
      attempts: 2,
      correct_count: 1,
      lapses: 1,
      streak: 0,
      last_correct: false,
      last_reviewed_at: "2026-09-11T00:00:00Z",
      due_at: "2026-09-11T00:10:00Z",
      is_due: true,
    },
  ],
};
beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(api.reviewLatest).mockResolvedValue(null);
  vi.mocked(api.reviewHistory).mockResolvedValue([]);
  vi.mocked(api.reviewMemory).mockResolvedValue({
    total: 0,
    due_count: 0,
    weak_count: 0,
    items: [],
    as_of: "2026-09-12T00:00:00Z",
    note: "",
  });
  vi.mocked(ragProviders).mockResolvedValue([
    { id: "deepseek", region: "default", model: "test" },
  ]);
  vi.mocked(api.reviewStart).mockResolvedValue({
    ...waiting,
    state: "ready",
    questions: [],
  });
  vi.mocked(api.reviewContinue).mockResolvedValue(waiting);
  vi.mocked(api.reviewRead).mockResolvedValue(waiting);
  vi.mocked(api.reviewCancel).mockResolvedValue();
  vi.mocked(mcpListServers).mockResolvedValue([]);
});
it("starts a due review and shows actual counts without inventing mastery", async () => {
  vi.mocked(api.reviewMemory).mockResolvedValue(dueMemory);
  render(<ReviewAgentPanel {...props()} />);
  const user = userEvent.setup();
  await screen.findByText("待复习 1");
  await user.click(screen.getByText("知识点与复习时间"));
  expect(screen.getByText(/已答 2 次，答对 1 次/)).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "复习到期知识点" }));
  expect(api.reviewStart).toHaveBeenCalledWith(
    expect.objectContaining({ due_only: true, kb: "book", version: "v1" }),
  );
  expect(api.reviewAnswer).not.toHaveBeenCalled();
});

it("starts with the selected question count and textbook requirement", async () => {
  render(<ReviewAgentPanel {...props()} />);
  const user = userEvent.setup();
  await waitFor(() =>
    expect(screen.getByRole("button", { name: "开始复习" })).toBeEnabled(),
  );
  await user.selectOptions(screen.getByLabelText("本次题目数"), "2");
  await user.selectOptions(screen.getByLabelText("资料依据"), "required");
  await user.click(screen.getByRole("button", { name: "开始复习" }));
  await waitFor(() =>
    expect(api.reviewStart).toHaveBeenCalledWith(
      expect.objectContaining({ question_count: 2, require_sources: true }),
    ),
  );
});

it("snapshots only the MCP servers explicitly allowed for this review", async () => {
  vi.mocked(mcpListServers).mockResolvedValue([
    {
      id: "study-tools",
      name: "Study Tools",
      transport: {
        transport: "streamable_http",
        url: "https://example.com/mcp",
      },
      enabled: true,
      credential_configured: false,
      key_last4: null,
      status: "connected",
      protocol_version: "2025-11-25",
      capabilities: { tools: {} },
      instructions: null,
      last_error: null,
      last_connected_at: "2026-09-26T00:00:00Z",
      created_at: "2026-09-26T00:00:00Z",
      updated_at: "2026-09-26T00:00:00Z",
      items: [
        {
          kind: "tool",
          name: "lookup",
          enabled: true,
          schema_hash: "hash",
          record: { name: "lookup", inputSchema: { type: "object" } },
          discovered_at: "2026-09-26T00:00:00Z",
        },
      ],
    },
  ]);
  const user = userEvent.setup();
  render(<ReviewAgentPanel {...props()} />);

  const server = await screen.findByRole("checkbox", {
    name: "Study Tools · 1 个工具",
  });
  expect(server).not.toBeChecked();
  await user.click(server);
  await user.click(screen.getByRole("button", { name: "开始复习" }));

  await waitFor(() =>
    expect(api.reviewStart).toHaveBeenCalledWith(
      expect.objectContaining({ mcp_server_ids: ["study-tools"] }),
    ),
  );
});
it("distinguishes budget estimates and workflow checks from content correctness", async () => {
  vi.mocked(api.reviewLatest).mockResolvedValue(waiting);
  vi.mocked(api.reviewTrace).mockResolvedValue({
    schema_version: "review-trace-v1",
    prompt_version: "review-agent-v3",
    run: {
      id: waiting.id,
      kb: "book",
      version: "v1",
      goal: waiting.goal,
      state: waiting.state,
      created_at: waiting.created_at,
      provider: waiting.provider,
      model: waiting.model,
      question_count: 1,
    },
    model_requests: 4,
    tool_calls: 4,
    reported_total_tokens: 120,
    usage_reported_requests: 4,
    events: [],
    harness: {
      version: "review-harness-v1",
      policy: {
        max_model_calls: 16,
        max_tool_calls: 32,
        max_token_charge: 400000,
        max_active_ms: 300000,
      },
      charged_tokens: 900,
      charged_active_ms: 300,
      stop_reason: null,
      completion_repairs: 1,
      context: {
        input_units: 8000,
        full_input_units: 35000,
        max_input_units: 24000,
        retained_history_messages: 6,
        omitted_history_messages: 20,
        compacted: true,
        estimate_method: "utf8_bytes_plus_protocol_allowance_not_tokenizer",
      },
      completion: {
        version: "review-completion-v1",
        outcome: "awaiting_answer",
        verified_submissions: 0,
        content_quality: "not_assessed",
        checks: [
          {
            code: "answers_recorded",
            label: "所有题目均已提交并记录",
            status: "pending",
          },
        ],
      },
    },
  });
  render(<ReviewAgentPanel {...props()} />);
  await screen.findByText("存储器保存什么？", { selector: "h3" });
  const user = userEvent.setup();
  await user.click(screen.getByText("执行记录"));
  expect(await screen.findByText(/预算占用含估算/)).toBeInTheDocument();
  expect(screen.getByText(/省略 20 条/)).toBeInTheDocument();
  await user.click(screen.getByText(/完成条件检查/));
  expect(
    screen.getByText("所有题目均已提交并记录：待满足"),
  ).toBeInTheDocument();
  expect(screen.getByText(/内容事实正确性待核验/)).toBeInTheDocument();
});

it("keeps an empty due queue disabled and discards a stale chapter result", async () => {
  let finish!: (value: api.ReviewMemoryOverview) => void;
  vi.mocked(api.reviewMemory).mockImplementationOnce(
    () =>
      new Promise((resolve) => {
        finish = resolve;
      }),
  );
  render(<ReviewAgentPanel {...props()} />);
  const user = userEvent.setup();
  await user.selectOptions(screen.getByLabelText("复习章节范围"), "c1");
  await screen.findByText("待复习 0");
  await act(async () => {
    finish(dueMemory);
    await Promise.resolve();
  });
  expect(screen.getByText("待复习 0")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "复习到期知识点" })).toBeDisabled();
  expect(api.reviewMemory).toHaveBeenLastCalledWith("book", "v1", "c1");
});
it("starts the agent in the chosen scope and waits for real input before grading", async () => {
  const p = props();
  render(<ReviewAgentPanel {...p} />);
  const user = userEvent.setup();
  await waitFor(() =>
    expect(screen.getByRole("button", { name: "开始复习" })).toBeEnabled(),
  );
  expect(api.reviewContinue).not.toHaveBeenCalled();
  await user.selectOptions(screen.getByLabelText("复习章节范围"), "c1");
  await user.click(screen.getByRole("button", { name: "开始复习" }));
  await screen.findByText("存储器保存什么？", { selector: "h3" });
  expect(api.reviewStart).toHaveBeenCalledWith(
    expect.objectContaining({
      kb: "book",
      version: "v1",
      chapter: "c1",
      provider: "deepseek",
    }),
  );
  expect(screen.getByRole("button", { name: "提交答案" })).toBeDisabled();
  expect(api.reviewAnswer).not.toHaveBeenCalled();
  expect(screen.queryByText(/参考答案/)).not.toBeInTheDocument();
  const answer = {
    ...waiting,
    state: "ready" as const,
    questions: [{ ...waiting.questions[0]!, selected_index: 1 }],
  };
  vi.mocked(api.reviewAnswer).mockResolvedValue(answer);
  vi.mocked(api.reviewContinue).mockResolvedValue({
    ...answer,
    state: "completed",
    questions: [
      {
        ...answer.questions[0]!,
        correct: false,
        correct_index: 0,
        explanation: "程序和数据都可以保存在存储器中。",
      },
    ],
  });
  await user.click(screen.getByRole("radio", { name: "只有图片" }));
  await user.click(screen.getByRole("button", { name: "提交答案" }));
  await screen.findByText(/这个知识点还需要巩固/);
  expect(api.reviewAnswer).toHaveBeenCalledExactlyOnceWith("r1", "q1", 1);
  expect(screen.getByText("参考答案：程序和数据")).toBeInTheDocument();
  vi.mocked(ragRelatedSources).mockResolvedValue({
    ...waiting.scope,
    query: waiting.questions[0]!.question,
    source_sha256: "sha",
    evidence: waiting.sources,
    text_chars: 20,
    status: "candidates",
    sha256: "packet",
  });
  await user.click(screen.getByText("相关原文 · Top 5"));
  await user.click(
    await screen.findByRole("button", { name: "查看第 3 页原文 ↗" }),
  );
  expect(ragRelatedSources).toHaveBeenCalledWith({
    kb: "book",
    version: "v1",
    chapter: "c1",
    query: "存储器保存什么？",
  });
  expect(p.onPage).toHaveBeenCalledWith(3, "v1");
});
it("restores a waiting question without issuing another model call", async () => {
  vi.mocked(api.reviewLatest).mockResolvedValue(waiting);
  render(<ReviewAgentPanel {...props()} />);
  const question = await screen.findByRole("group", {
    name: "存储器保存什么？",
  });
  expect(within(question).getAllByRole("radio")).toHaveLength(2);
  expect(api.reviewStart).not.toHaveBeenCalled();
  expect(api.reviewContinue).not.toHaveBeenCalled();
});

it("shows only the selectable question when starting a new review with repeated model text", async () => {
  const repeated = {
    ...waiting,
    output: "下面是一道选择题。\n存储器保存什么？\nA. 程序和数据\nB. 只有图片",
  };
  vi.mocked(api.reviewLatest).mockResolvedValue(repeated);
  vi.mocked(api.reviewContinue).mockResolvedValue(repeated);
  render(<ReviewAgentPanel {...props()} />);
  const user = userEvent.setup();
  await user.click(await screen.findByRole("button", { name: "开始新的复习" }));
  await screen.findByText("轮到你了");
  expect(screen.queryByText(/下面是一道选择题/)).not.toBeInTheDocument();
  expect(
    screen.getAllByRole("heading", { name: "存储器保存什么？" }),
  ).toHaveLength(1);
  expect(screen.getAllByRole("radio")).toHaveLength(2);
  await user.click(screen.getByRole("radio", { name: "程序和数据" }));
  expect(screen.getByRole("radio", { name: "程序和数据" })).toBeChecked();
  expect(screen.getByRole("button", { name: "提交答案" })).toBeEnabled();
});

it("retrieves five originals for the selected question instead of accumulated search history", async () => {
  const run = structuredClone(waiting);
  run.questions.push({
    ...run.questions[0]!,
    id: "q2",
    question: "Cache 为什么更快？",
  });
  run.sources = Array.from({ length: 24 }, (_, index) => ({
    ...waiting.sources[0]!,
    id: `S${index + 1}`,
    block_id: `b${index}`,
    text: `历史检索 ${index}`,
  }));
  vi.mocked(api.reviewLatest).mockResolvedValue(run);
  vi.mocked(ragRelatedSources).mockImplementation((request) =>
    Promise.resolve({
      ...request,
      filename: "教材.pdf",
      source_sha256: "source",
      sha256: "packet",
      text_chars: 50,
      status: "candidates",
      evidence: Array.from({ length: 5 }, (_, index) => ({
        ...waiting.sources[0]!,
        block_id: `b${index}`,
        text: `${request.query} 原文 ${index}`,
      })),
    }),
  );
  render(<ReviewAgentPanel {...props()} />);
  const user = userEvent.setup();
  await user.click(await screen.findByText("相关原文 · Top 5"));
  await screen.findByText("Cache 为什么更快？ 原文 0");
  expect(document.querySelectorAll("blockquote")).toHaveLength(5);
  expect(screen.queryByText(/历史检索/)).not.toBeInTheDocument();
  await user.selectOptions(screen.getByLabelText("原文对应题目"), "r1/q1");
  await screen.findByText("存储器保存什么？ 原文 0");
  expect(
    screen.queryByText("Cache 为什么更快？ 原文 0"),
  ).not.toBeInTheDocument();
  expect(document.querySelectorAll("blockquote")).toHaveLength(5);
  expect(api.reviewContinue).not.toHaveBeenCalled();
});
it("prevents duplicate starts and can request pause while the model is running", async () => {
  let finish!: (run: ReviewRun) => void;
  vi.mocked(api.reviewContinue).mockImplementationOnce(
    () =>
      new Promise((resolve) => {
        finish = resolve;
      }),
  );
  render(<ReviewAgentPanel {...props()} />);
  await waitFor(() =>
    expect(screen.getByRole("button", { name: "开始复习" })).toBeEnabled(),
  );
  const user = userEvent.setup();
  await user.dblClick(screen.getByRole("button", { name: "开始复习" }));
  expect(api.reviewStart).toHaveBeenCalledTimes(1);
  await user.click(await screen.findByRole("button", { name: "暂停复习" }));
  expect(api.reviewCancel).toHaveBeenCalledExactlyOnceWith("r1");
  await act(async () => {
    finish({ ...waiting, state: "paused", error: "已暂停" });
    await Promise.resolve();
  });
  expect(await screen.findByRole("button", { name: "继续复习" })).toBeEnabled();
});
it("preserves saved questions when continuing a failed task", async () => {
  vi.mocked(api.reviewLatest).mockResolvedValue({
    ...waiting,
    state: "failed",
    error: "连接中断",
  });
  render(<ReviewAgentPanel {...props()} />);
  await userEvent
    .setup()
    .click(await screen.findByRole("button", { name: "继续复习" }));
  await screen.findByText("轮到你了");
  expect(api.reviewContinue).toHaveBeenCalledExactlyOnceWith("r1");
  expect(api.reviewStart).not.toHaveBeenCalled();
});
it("shows configuration errors without creating a task", async () => {
  vi.mocked(ragProviders).mockResolvedValue([]);
  render(<ReviewAgentPanel {...props()} />);
  await waitFor(() =>
    expect(screen.getByRole("button", { name: "开始复习" })).toBeEnabled(),
  );
  await userEvent
    .setup()
    .click(screen.getByRole("button", { name: "开始复习" }));
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "请先在模型设置中配置并测试模型通道",
  );
  expect(api.reviewStart).not.toHaveBeenCalled();
});

it("opens the trace on demand and exports through the native save dialog", async () => {
  vi.mocked(api.reviewLatest).mockResolvedValue(waiting);
  const trace: api.ReviewTrace = {
    schema_version: "review-trace-v1",
    prompt_version: "review-agent-v1",
    run: {
      id: "r1",
      kb: "book",
      version: "v1",
      goal: "复习",
      state: "waiting_answer",
      created_at: waiting.created_at,
      provider: "deepseek",
      model: "test",
      question_count: 1,
    },
    model_requests: 2,
    tool_calls: 1,
    reported_total_tokens: 60,
    usage_reported_requests: 2,
    events: [
      {
        sequence: 1,
        kind: "tool",
        name: "search_textbook",
        status: "succeeded",
        started_at: waiting.created_at,
        finished_at: waiting.created_at,
        duration_ms: 5,
        details: {
          arguments: { query: "存储器" },
          result: { matches: [{ source_id: "S1", page: 3 }] },
        },
        usage: null,
      },
    ],
  };
  vi.mocked(api.reviewTrace).mockResolvedValue(trace);
  vi.mocked(api.reviewExport).mockResolvedValue("D:\\review.json");
  render(<ReviewAgentPanel {...props()} />);
  const user = userEvent.setup();
  await screen.findByText("轮到你了");
  expect(api.reviewTrace).not.toHaveBeenCalled();
  await user.click(screen.getByText("执行记录"));
  await screen.findByText("检索教材");
  expect(api.reviewTrace).toHaveBeenCalledWith("r1");
  expect(
    screen.getByText("已报告 60 tokens（2/2 次请求）"),
  ).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "导出 JSON" }));
  expect(api.reviewExport).toHaveBeenCalledExactlyOnceWith("r1");
  expect(
    await screen.findByText("已导出到：D:\\review.json"),
  ).toBeInTheDocument();
});

it("loads an earlier review without calling the model or retaining an unsent choice", async () => {
  vi.mocked(api.reviewLatest).mockResolvedValue(waiting);
  vi.mocked(api.reviewHistory).mockResolvedValue(
    ["r1", "r0"].map((id) => ({
      id,
      kb: "book",
      version: "v1",
      goal: id,
      state: "waiting_answer",
      created_at: waiting.created_at,
      provider: "deepseek",
      model: "test",
      question_count: 1,
    })),
  );
  vi.mocked(api.reviewRead).mockResolvedValue({
    ...waiting,
    id: "r0",
    goal: "r0",
  });
  render(<ReviewAgentPanel {...props()} />);
  const user = userEvent.setup();
  await user.click(await screen.findByRole("radio", { name: "只有图片" }));
  expect(screen.getByRole("button", { name: "提交答案" })).toBeEnabled();
  await user.selectOptions(screen.getByLabelText("近期复习"), "r0");
  await waitFor(() =>
    expect(screen.getByRole("button", { name: "提交答案" })).toBeDisabled(),
  );
  expect(api.reviewRead).toHaveBeenCalledWith("r0");
  expect(api.reviewContinue).not.toHaveBeenCalled();
});
