import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { vi } from "vitest";
import { KnowledgeBaseScreen } from "./KnowledgeBaseScreen";
import * as kb from "../lib/knowledgeBase";
import type { Catalog, PdfKnowledgeBase } from "../lib/knowledgeBase";
import * as learning from "../lib/learning";

vi.mock("../lib/knowledgeBase", () => ({
  kbRead: vi.fn(),
  choosePdf: vi.fn(),
  importPdf: vi.fn(),
  pausePdf: vi.fn(),
  resumePdf: vi.fn(),
}));
vi.mock("../lib/autoHideGuard", () => ({ useAutoHideGuard: vi.fn() }));
vi.mock("../lib/learning", async (load) => ({
  ...(await load<typeof learning>()),
  learningResume: vi.fn(),
}));

const book: PdfKnowledgeBase = {
  id: "computer-organization",
  filename: "计算机组成原理.pdf",
  pages: 436,
  chunks: 1267,
  status: "partial_ready",
  version: "v1",
  coverage: { eligible_fraction_of_recognized_body: 0.605 },
  job: null,
};
const passage = {
  id: "p1",
  text: "总线是连接多个部件的信息传输线。",
  chapter_path: ["第三章 系统总线"],
  locations: [{ page: 50 }],
};
let catalog: Catalog;

beforeEach(() => {
  vi.clearAllMocks();
  catalog = {
    items: [book],
    errors: [],
    models_ready: true,
    import_running: false,
  };
  vi.mocked(kb.kbRead).mockImplementation((request) => {
    if (request.op === "catalog") return Promise.resolve(catalog);
    if (request.op === "chapters")
      return Promise.resolve([
        {
          id: "c3",
          title: "第三章 系统总线",
          start_page: 47,
          depth: 0,
          kind: "chapter",
        },
      ]);
    if (request.op === "search") return Promise.resolve({ results: [passage] });
    if (request.op === "page")
      return Promise.resolve({
        page: request.page,
        printed_label: "44",
        image: "data:image/png;base64,AA==",
      });
    return Promise.reject(new Error("Unexpected request"));
  });
});

it("opens in search with only the retained tabs and follows a chapter-filtered citation", async () => {
  const user = userEvent.setup();
  render(<KnowledgeBaseScreen />);
  await user.click(
    await screen.findByRole("button", { name: /计算机组成原理.pdf/ }),
  );
  expect(
    within(screen.getByLabelText("资料查看方式"))
      .getAllByRole("button")
      .map((button) => button.textContent),
  ).toEqual(["检索", "随机学习", "复习 Agent", "原文"]);
  expect(screen.getByRole("button", { name: "检索" })).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  expect(screen.queryByText(/问答与学习/)).not.toBeInTheDocument();
  expect(kb.kbRead).not.toHaveBeenCalledWith(
    expect.objectContaining({ op: "browse" }),
  );
  expect(kb.kbRead).not.toHaveBeenCalledWith(
    expect.objectContaining({ op: "search" }),
  );
  expect(screen.getByText(/60.5%/)).toBeInTheDocument();
  await user.selectOptions(screen.getByLabelText("章节范围"), "c3");
  await user.type(screen.getByLabelText("检索内容"), "总线");
  await user.click(screen.getByRole("button", { name: "检索原文" }));
  expect(await screen.findByText(passage.text)).toBeInTheDocument();
  await waitFor(() =>
    expect(kb.kbRead).toHaveBeenCalledWith({
      op: "search",
      kb: book.id,
      query: "总线",
      mode: "hybrid",
      chapter: "c3",
    }),
  );
  await user.click(
    await screen.findByRole("button", { name: /查看第 50 页原文/ }),
  );
  expect(
    await screen.findByAltText("计算机组成原理.pdf 第 50 页原文"),
  ).toBeInTheDocument();
  expect(screen.getByRole("spinbutton", { name: "原文页码" })).toHaveValue(50);
});

it("retains random learning progress and chapter scope while switching to the original pages", async () => {
  vi.mocked(learning.learningResume).mockResolvedValue({
    session: {
      id: "session1",
      kb: book.id,
      chapter: "c3",
      chapter_path: ["第三章 系统总线"],
      filter: "review",
      relaxed: false,
      history: [],
      cursor: null,
      revision: 1,
    },
    card: null,
    state: null,
    reason: null,
    summary: {
      new: 1,
      learning: 2,
      review: 3,
      mastered: 4,
      today: 2,
      covered_units: 5,
    },
  });
  const user = userEvent.setup();
  render(<KnowledgeBaseScreen />);
  await user.click(
    await screen.findByRole("button", { name: /计算机组成原理.pdf/ }),
  );
  expect(learning.learningResume).not.toHaveBeenCalled();
  await user.click(screen.getByRole("button", { name: "随机学习" }));
  expect(await screen.findByText("今日已学 2 张")).toBeInTheDocument();
  expect(screen.queryByLabelText("章节范围")).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "学习设置" }));
  expect(await screen.findByLabelText("本知识库学习统计")).toHaveTextContent(
    "需复习 3",
  );
  expect(screen.getByLabelText("章节范围")).toHaveValue("c3");
  expect(screen.getByLabelText("学习范围筛选")).toHaveValue("review");
  await user.selectOptions(screen.getByLabelText("学习范围筛选"), "all");

  await user.click(screen.getByRole("button", { name: "原文" }));
  expect(
    await screen.findByAltText("计算机组成原理.pdf 第 1 页原文"),
  ).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "上一页原文" })).toBeDisabled();
  await user.click(screen.getByRole("button", { name: "下一页原文" }));
  expect(
    await screen.findByAltText("计算机组成原理.pdf 第 2 页原文"),
  ).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "放大查看" }));
  expect(screen.getByRole("button", { name: "适应宽度" })).toBeInTheDocument();

  await user.click(screen.getByRole("button", { name: "随机学习" }));
  expect(screen.getByLabelText("学习范围筛选")).toHaveValue("all");
  expect(screen.getByLabelText("章节范围")).toHaveValue("c3");
  expect(learning.learningResume).toHaveBeenCalledExactlyOnceWith(book.id);
  await user.click(screen.getByRole("button", { name: "检索" }));
  expect(screen.getByLabelText("章节范围")).toHaveValue("c3");
  expect(screen.getByLabelText("检索内容")).toBeInTheDocument();
});

it("selects a PDF then explicitly starts import; repeat clicks cannot duplicate the task", async () => {
  const user = userEvent.setup();
  render(<KnowledgeBaseScreen />);
  vi.mocked(kb.choosePdf).mockResolvedValue({
    path: "D:\\新 教材.pdf",
    filename: "新 教材.pdf",
    pages: 12,
    bytes: 20000,
  });
  let complete!: (value: { kb: string; job: string; reused: boolean }) => void;
  vi.mocked(kb.importPdf).mockImplementation(
    () =>
      new Promise((resolve) => {
        complete = resolve;
      }),
  );
  await user.click(await screen.findByRole("button", { name: "选择 PDF" }));
  expect(kb.importPdf).not.toHaveBeenCalled();
  await user.clear(screen.getByLabelText("正文起始页"));
  await user.type(screen.getByLabelText("正文起始页"), "3");
  await user.click(screen.getByRole("button", { name: "开始导入" }));
  expect(screen.getByRole("button", { name: "正在准备…" })).toBeDisabled();
  expect(kb.importPdf).toHaveBeenCalledExactlyOnceWith("D:\\新 教材.pdf", 3);
  await act(async () => {
    complete({ kb: book.id, job: "job1", reused: true });
    await Promise.resolve();
  });
  expect(await screen.findByText(/无需重复处理/)).toBeInTheDocument();
});

it("shows an empty library and leaves it unchanged when file selection is cancelled", async () => {
  catalog.items = [];
  vi.mocked(kb.choosePdf).mockResolvedValue(null);
  const user = userEvent.setup();
  render(<KnowledgeBaseScreen />);
  expect(
    await screen.findByText("第一份资料，从一本书开始"),
  ).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "选择 PDF" }));
  expect(kb.importPdf).not.toHaveBeenCalled();
  expect(
    screen.queryByRole("button", { name: "开始导入" }),
  ).not.toBeInTheDocument();
});

it("displays checkpoint progress and allows pausing and resuming", async () => {
  catalog.items = [
    {
      ...book,
      version: null,
      chunks: 0,
      status: "not_ready",
      job: {
        id: "job1",
        status: "running",
        stage: "extracting",
        total: 436,
        completed: 21,
        counts: { needs_review: 3 },
        error: null,
        page_errors: [],
      },
    },
  ];
  catalog.import_running = true;
  const user = userEvent.setup();
  render(<KnowledgeBaseScreen />);
  await user.click(
    await screen.findByRole("button", { name: /计算机组成原理.pdf/ }),
  );
  expect(screen.getByRole("progressbar")).toHaveAttribute("value", "21");
  await user.click(screen.getByRole("button", { name: "暂停处理" }));
  expect(kb.pausePdf).toHaveBeenCalledWith(book.id, "job1");
  catalog = {
    ...catalog,
    import_running: false,
    items: [
      {
        ...catalog.items[0]!,
        job: { ...catalog.items[0]!.job!, status: "paused" },
      },
    ],
  };
  await user.click(screen.getByRole("button", { name: "← 全部知识库" }));
  await user.click(screen.getByRole("button", { name: "刷新" }));
  await user.click(
    await screen.findByRole("button", { name: /计算机组成原理.pdf/ }),
  );
  await user.click(await screen.findByRole("button", { name: "继续处理" }));
  expect(kb.resumePdf).toHaveBeenCalledWith(book.id, "job1");
});

it("retrieves evidence with the selected mode and allows retry after errors", async () => {
  const user = userEvent.setup();
  render(<KnowledgeBaseScreen />);
  await user.click(
    await screen.findByRole("button", { name: /计算机组成原理.pdf/ }),
  );
  await user.click(screen.getByRole("button", { name: "检索" }));
  await user.type(screen.getByLabelText("检索内容"), "总线的作用是什么？");
  await user.selectOptions(screen.getByLabelText("检索方式"), "keyword");
  await user.click(screen.getByRole("button", { name: "检索原文" }));
  expect(await screen.findByText(passage.text)).toBeInTheDocument();
  expect(kb.kbRead).toHaveBeenCalledWith({
    op: "search",
    kb: book.id,
    query: "总线的作用是什么？",
    mode: "keyword",
    chapter: null,
  });
  vi.mocked(kb.kbRead).mockRejectedValueOnce(new Error("模型校验失败"));
  await user.click(screen.getByRole("button", { name: "检索原文" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("模型校验失败");
  await user.click(screen.getByRole("button", { name: "重新加载" }));
  expect(await screen.findByText(passage.text)).toBeInTheDocument();
});

it("discards a late search response after the chapter scope changes", async () => {
  const user = userEvent.setup();
  render(<KnowledgeBaseScreen />);
  await user.click(
    await screen.findByRole("button", { name: /计算机组成原理.pdf/ }),
  );
  await user.click(screen.getByRole("button", { name: "检索" }));
  await user.type(screen.getByLabelText("检索内容"), "总线");
  let complete!: (value: { results: (typeof passage)[] }) => void;
  vi.mocked(kb.kbRead).mockImplementationOnce(
    () =>
      new Promise((resolve) => {
        complete = resolve;
      }),
  );
  await user.click(screen.getByRole("button", { name: "检索原文" }));
  await user.selectOptions(screen.getByLabelText("章节范围"), "c3");
  await act(async () => {
    complete({ results: [passage] });
    await Promise.resolve();
  });
  expect(screen.queryByText(passage.text)).not.toBeInTheDocument();
  expect(screen.getByRole("button", { name: "检索原文" })).toBeEnabled();
});

it("reports runtime failure without pretending to import files", async () => {
  vi.mocked(kb.kbRead).mockRejectedValue(new Error("本机 PDF 处理组件未就绪"));
  render(<KnowledgeBaseScreen />);
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "本机 PDF 处理组件未就绪",
  );
  expect(screen.getByRole("button", { name: "选择 PDF" })).toBeDisabled();
});
