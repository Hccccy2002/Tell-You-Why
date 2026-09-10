import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { vi } from "vitest";
import { LearningPanel } from "./LearningPanel";
import * as api from "../lib/learning";
import type { LearningView } from "../lib/learning";
import type { PdfKnowledgeBase } from "../lib/knowledgeBase";
vi.mock("../lib/learning", async (load) => ({
  ...(await load<typeof api>()),
  learningResume: vi.fn(),
  learningStart: vi.fn(),
  learningNext: vi.fn(),
  learningPrevious: vi.fn(),
  learningRecord: vi.fn(),
  learningReset: vi.fn(),
}));
const book: PdfKnowledgeBase = {
  id: "book",
  filename: "教材.pdf",
  version: "v1",
  pages: 10,
  status: "ready",
  chunks: 10,
  coverage: null,
  job: null,
};
const blank: LearningView = {
  session: null,
  card: null,
  state: null,
  reason: null,
  summary: {
    new: 1,
    learning: 0,
    review: 0,
    mastered: 0,
    today: 0,
    covered_units: 0,
  },
};
const ready: LearningView = {
  ...blank,
  session: {
    id: "s1",
    kb: "book",
    chapter: null,
    chapter_path: [],
    filter: "default",
    relaxed: false,
    history: [
      {
        id: "p1",
        card_id: "c1",
        confirmed: false,
        revealed: false,
        expires_at: "2099-01-01T00:00:00Z",
      },
    ],
    cursor: 0,
    revision: 1,
  },
  state: {
    card_id: "c1",
    status: "new",
    shown_count: 0,
    revealed_count: 0,
    revision: 0,
  },
  card: {
    id: "c1",
    kb: "book",
    kind: "card",
    state: "completed",
    provider: "test",
    region: "default",
    model: "m1",
    created_at: "2026-09-10T00:00:00Z",
    error: null,
    usage: [],
    packet: {
      kb: "book",
      version: "v1",
      filename: "教材.pdf",
      source_sha256: "sha",
      query: "存储器有什么作用？",
      chapter: null,
      evidence: [
        {
          id: "E1",
          text: "存储器存放程序和数据。它由多个存储单元组成。",
          block_id: "b1",
          page: 3,
          chapter_path: ["存储器"],
        },
      ],
      text_chars: 12,
      status: "candidates",
      sha256: "packet",
    },
    result: {
      status: "answered",
      question: "存储器有什么作用？",
      answer: [
        {
          text: "保存程序和数据。",
          citations: [{ evidence_id: "E1", quote: "存储器存放程序和数据。" }],
        },
      ],
      explanation: [
        {
          text: "可以把存储器理解为存放指令和数据的书架。",
          citations: [{ evidence_id: "E1", quote: "存储器存放程序和数据。" }],
        },
      ],
      reason: "",
    },
  },
};
let current: LearningView;
beforeEach(() => {
  vi.clearAllMocks();
  current = structuredClone(ready);
  vi.mocked(api.learningResume).mockResolvedValue(structuredClone(blank));
  vi.mocked(api.learningStart).mockResolvedValue({
    ...blank,
    session: { ...ready.session!, history: [], cursor: null },
  });
  vi.mocked(api.learningNext).mockImplementation(() =>
    Promise.resolve(structuredClone(current)),
  );
  vi.mocked(api.learningReset).mockResolvedValue(structuredClone(blank));
  vi.mocked(api.learningRecord).mockImplementation((event) => {
    const p = current.session!.history[0]!;
    if (event.kind === "shown") {
      p.confirmed = true;
      current.state!.shown_count = 1;
    }
    if (event.kind === "revealed") {
      p.revealed = true;
      current.state!.status = "learning";
      current.summary.today = 1;
    }
    if (event.kind === "status") current.state!.status = event.status!;
    if (event.kind === "undo") current.state!.status = "learning";
    current.state!.revision++;
    current.session!.revision++;
    return Promise.resolve(structuredClone(current));
  });
});
const props = () => ({
  book,
  chapter: "",
  chapters: [],
  active: true,
  onPage: vi.fn(),
  onRestoreChapter: vi.fn(),
  onChapterChange: vi.fn(),
});
async function begin() {
  const user = userEvent.setup();
  await waitFor(() =>
    expect(screen.getByRole("button", { name: "开始学习" })).toBeEnabled(),
  );
  await user.click(screen.getByRole("button", { name: "开始学习" }));
  await waitFor(() =>
    expect(screen.getByRole("button", { name: "揭晓答案" })).toBeEnabled(),
  );
  return user;
}
it("learns local cards with explicit reveal, reversible mastery and versioned citations", async () => {
  const p = props();
  render(<LearningPanel {...p} />);
  const user = await begin();
  expect(screen.queryByText("保存程序和数据。")).not.toBeInTheDocument();
  expect(screen.queryByLabelText("学习范围筛选")).not.toBeInTheDocument();
  expect(screen.queryByLabelText("本知识库学习统计")).not.toBeInTheDocument();
  expect(
    screen.queryByRole("button", { name: "原文依据" }),
  ).not.toBeInTheDocument();
  expect(api.learningRecord).toHaveBeenCalledTimes(1);
  await user.click(screen.getByRole("button", { name: "揭晓答案" }));
  await screen.findByText("保存程序和数据。");
  expect(screen.queryByText(/可以把存储器理解为/)).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "原文依据" }));
  expect(
    screen.getAllByText("存储器存放程序和数据。它由多个存储单元组成。"),
  ).toHaveLength(1);
  expect(screen.queryByText(/可以把存储器理解为/)).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "查看第 3 页原文 ↗" }));
  expect(p.onPage).toHaveBeenCalledWith(3, "v1");
  await user.click(screen.getByRole("button", { name: "AI 解释" }));
  expect(
    screen.getByText("可以把存储器理解为存放指令和数据的书架。"),
  ).toBeInTheDocument();
  expect(
    screen.queryByRole("region", { name: "原文依据" }),
  ).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "记住了" }));
  await screen.findByText("已掌握");
  expect(screen.getByRole("button", { name: "记住了" })).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  await user.click(screen.getByRole("button", { name: "撤销" }));
  await screen.findByText("学习中");
  const last = vi.mocked(api.learningRecord).mock.calls.at(-1)![0];
  expect(last.kind).toBe("undo");
  expect(typeof last.undo_event_id).toBe("string");
});
it("skips without recording learned state and shows exhausted choices", async () => {
  render(<LearningPanel {...props()} />);
  const user = await begin();
  vi.mocked(api.learningNext).mockResolvedValue({
    ...current,
    reason: "近期候选已用完",
  });
  await user.click(screen.getByRole("button", { name: "跳过 →" }));
  await screen.findByText(/近期候选已用完/);
  expect(
    vi
      .mocked(api.learningRecord)
      .mock.calls.some(([e]) => e.kind === "revealed" || e.kind === "status"),
  ).toBe(false);
  expect(
    screen.getByRole("button", { name: "复习近期卡片" }),
  ).toBeInTheDocument();
});
it("restores revealed progress without new shown events", async () => {
  current.session!.history[0]!.confirmed = true;
  current.session!.history[0]!.revealed = true;
  vi.mocked(api.learningResume).mockResolvedValue(current);
  render(<LearningPanel {...props()} />);
  await screen.findByText("保存程序和数据。");
  expect(api.learningRecord).not.toHaveBeenCalled();
});
it("keeps the open evidence when returning from the PDF, and collapses details for the next card", async () => {
  const p = props();
  const mounted = render(<LearningPanel {...p} />);
  const user = await begin();
  await user.click(screen.getByRole("button", { name: "揭晓答案" }));
  await user.click(screen.getByRole("button", { name: "原文依据" }));
  await user.click(screen.getByRole("button", { name: "查看第 3 页原文 ↗" }));
  mounted.rerender(<LearningPanel {...p} active={false} />);
  mounted.rerender(<LearningPanel {...p} />);
  expect(screen.getByRole("button", { name: "原文依据" })).toHaveAttribute(
    "aria-expanded",
    "true",
  );
  const next = structuredClone(current);
  next.session!.history.push({
    ...next.session!.history[0]!,
    id: "p2",
    card_id: "c2",
  });
  next.session!.cursor = 1;
  next.card!.id = "c2";
  next.card!.result!.question = "总线有什么作用？";
  vi.mocked(api.learningNext).mockResolvedValue(next);
  await user.click(screen.getByRole("button", { name: "下一张 →" }));
  expect(await screen.findByText("总线有什么作用？")).toBeInTheDocument();
  expect(
    screen.queryByRole("region", { name: "原文依据" }),
  ).not.toBeInTheDocument();
  expect(screen.getByRole("button", { name: "原文依据" })).toHaveAttribute(
    "aria-expanded",
    "false",
  );
});
it("does not substitute generated claims for missing original evidence or explanations", async () => {
  current.session!.history[0]!.confirmed = true;
  current.session!.history[0]!.revealed = true;
  current.card!.packet.evidence = [];
  current.card!.result!.explanation = [];
  vi.mocked(api.learningResume).mockResolvedValue(current);
  render(<LearningPanel {...props()} />);
  const user = userEvent.setup();
  await user.click(await screen.findByRole("button", { name: "原文依据" }));
  expect(
    screen.getByText("这张卡没有保存可查看的原文依据。"),
  ).toBeInTheDocument();
  expect(
    screen.queryByRole("button", { name: /查看第.*页原文/ }),
  ).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "AI 解释" }));
  expect(
    screen.getByText("这张卡暂未保存详细解释，可先查看原文依据。"),
  ).toBeInTheDocument();
  expect(api.learningRecord).not.toHaveBeenCalled();
});
it("applies learning filters from the collapsed settings and returns to the card", async () => {
  render(<LearningPanel {...props()} />);
  const user = await begin();
  await user.click(screen.getByRole("button", { name: "学习设置" }));
  await user.selectOptions(screen.getByLabelText("学习范围筛选"), "review");
  current.session!.filter = "review";
  await user.click(screen.getByRole("button", { name: "应用并换一张" }));
  expect(api.learningStart).toHaveBeenLastCalledWith({
    kb: "book",
    chapter: null,
    chapter_path: [],
    filter: "review",
    relaxed: false,
  });
  await waitFor(() =>
    expect(screen.getByRole("button", { name: "学习设置" })).toHaveAttribute(
      "aria-expanded",
      "false",
    ),
  );
  expect(screen.queryByLabelText("学习范围筛选")).not.toBeInTheDocument();
});
it("confirms only visible cards and retains the question when saving fails", async () => {
  vi.mocked(api.learningResume).mockResolvedValue(current);
  const p = props();
  const mounted = render(<LearningPanel {...p} active={false} />);
  await screen.findByText("存储器有什么作用？");
  expect(api.learningRecord).not.toHaveBeenCalled();
  vi.mocked(api.learningRecord).mockRejectedValue(new Error("保存失败"));
  mounted.rerender(<LearningPanel {...p} />);
  await screen.findByRole("alert");
  expect(screen.getByRole("button", { name: "揭晓答案" })).toBeDisabled();
  expect(api.learningRecord).toHaveBeenCalledTimes(1);
});
it("requires the application confirmation before resetting and keeps cards on cancel", async () => {
  render(<LearningPanel {...props()} />);
  const user = await begin();
  await user.click(screen.getByRole("button", { name: "学习设置" }));
  await user.click(screen.getByRole("button", { name: "重置学习记录" }));
  expect(api.learningReset).not.toHaveBeenCalled();
  await user.click(screen.getByRole("button", { name: "取消" }));
  expect(screen.getByText("存储器有什么作用？")).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "重置学习记录" }));
  await user.click(
    within(screen.getByRole("dialog")).getByRole("button", {
      name: "重置学习记录",
    }),
  );
  await waitFor(() => expect(api.learningReset).toHaveBeenCalledWith("book"));
  await waitFor(() =>
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument(),
  );
});
it("builds nested chapter paths without mixing sibling branches", () => {
  expect(
    api.chapterPath(
      [
        { id: "a", title: "A", depth: 0, kind: "chapter", start_page: 1 },
        { id: "a1", title: "A1", depth: 1, kind: "section", start_page: 2 },
        { id: "b", title: "B", depth: 0, kind: "chapter", start_page: 3 },
        { id: "b1", title: "B1", depth: 1, kind: "section", start_page: 4 },
      ],
      "b1",
    ),
  ).toEqual(["B", "B1"]);
});
