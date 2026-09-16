import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { vi } from "vitest";
import { StudyPanel } from "./StudyPanel";
import type { StudyClient, StudySession, StudyHighlight } from "../lib/study";
import type { ProviderSpec } from "../types";
import { fallbackCards } from "../data/fallbackCards";

it("returns a restored session to its real source card and keeps its expanded view", async () => {
  const user = userEvent.setup();
  const api = client(
    session({
      source_card: {
        card_id: fallbackCards[0]!.id,
        title: fallbackCards[0]!.question,
      },
      source_expanded: true,
    }),
  );
  vi.mocked(api.sourceCard).mockResolvedValue(fallbackCards[0]!);
  const onOpenCard = vi.fn();
  render(
    <StudyPanel
      topics={[]}
      providers={[provider]}
      client={api}
      sessionId="study"
      onExit={vi.fn()}
      onModelSettings={vi.fn()}
      onOpenCard={onOpenCard}
    />,
  );
  await user.click(await screen.findByRole("button", { name: "回到原卡" }));
  expect(api.pause).toHaveBeenCalledWith("study", false);
  expect(api.sourceCard).toHaveBeenCalledWith("study");
  expect(onOpenCard).toHaveBeenCalledWith(fallbackCards[0], true);
  expect(api.continue).not.toHaveBeenCalled();
});

it("keeps the saved lesson accessible when its source card has been deleted", async () => {
  const api = client(
    session({
      state: "completed",
      source_card: { card_id: "deleted", title: "已删除的卡" },
    }),
  );
  const onOpenCard = vi.fn();
  render(
    <StudyPanel
      topics={[]}
      providers={[]}
      client={api}
      onExit={vi.fn()}
      onModelSettings={vi.fn()}
      onOpenCard={onOpenCard}
    />,
  );
  await userEvent
    .setup()
    .click(await screen.findByRole("button", { name: "回到原卡" }));
  expect(await screen.findByText(/原卡已删除或不存在/)).toBeVisible();
  expect(screen.getByRole("region", { name: "学习小结" })).toBeVisible();
  expect(onOpenCard).not.toHaveBeenCalled();
  expect(api.pause).not.toHaveBeenCalled();
});

const provider: ProviderSpec = {
  id: "deepseek",
  label: "DeepSeek",
  regions: [],
  models: [],
  selectedRegion: "default",
  selectedModel: "test",
  keyConfigured: true,
  keyLast4: "test",
  connectionVerified: true,
};
function session(overrides: Partial<StudySession> = {}): StudySession {
  return {
    id: "study",
    goal: "理解 DNS",
    topic: "网络",
    provider: "deepseek",
    model: "test",
    state: "waiting",
    revision: 2,
    created_at: "2026-09-16",
    error: null,
    next_topic: null,
    can_resume: false,
    review_target: null,
    source_card: null,
    questions: [],
    can_ask: true,
    question_limit: 4,
    summary: { topics: ["DNS 是什么"], answered: 0, correct: 0 },
    steps: [
      {
        id: "one",
        kind: "concept",
        title: "DNS 是什么",
        text: "DNS 帮助我们查找域名对应的地址。",
        reason: "从最基础的用途开始。",
        card_id: null,
        feedback: null,
        quiz: null,
      },
    ],
    ...overrides,
  };
}
function client(latest: StudySession | null = null): StudyClient {
  return {
    ask: vi.fn(),
    questionFeedback: vi.fn(),
    startDoubt: vi.fn(),
    highlights: vi.fn().mockResolvedValue([]),
    saveHighlight: vi.fn().mockResolvedValue("saved"),
    removeHighlight: vi.fn().mockResolvedValue(undefined),
    cardSessions: vi.fn().mockResolvedValue([]),
    sourceCard: vi.fn().mockResolvedValue(null),
    startCard: vi.fn().mockResolvedValue(
      session({
        state: "ready",
        steps: [],
        can_resume: true,
        source_card: { card_id: "dns-card", title: "为什么需要 DNS？" },
      }),
    ),
    home: vi.fn().mockResolvedValue({ active: null }),
    startReview: vi.fn().mockResolvedValue(
      session({
        state: "ready",
        steps: [],
        can_resume: true,
        review_target: { concept_key: "dns", title: "DNS" },
      }),
    ),
    history: vi.fn().mockResolvedValue(latest ? [latest] : []),
    reset: vi.fn().mockResolvedValue(undefined),
    latest: vi.fn().mockResolvedValue(latest),
    start: vi
      .fn()
      .mockResolvedValue(
        session({ state: "ready", steps: [], revision: 0, can_resume: true }),
      ),
    read: vi.fn().mockResolvedValue(latest),
    continue: vi.fn().mockResolvedValue(session()),
    feedback: vi
      .fn()
      .mockResolvedValue(
        session({ state: "ready", revision: 3, can_resume: true }),
      ),
    pause: vi
      .fn()
      .mockResolvedValue(
        session({ state: "paused", revision: 5, can_resume: true }),
      ),
  };
}
function show(api: StudyClient, onExit = vi.fn()) {
  return render(
    <StudyPanel
      topics={[]}
      providers={[provider]}
      onExit={onExit}
      onModelSettings={vi.fn()}
      client={api}
    />,
  );
}

const pendingQuestion = {
  id: "00000000-0000-4000-8000-000000000001",
  step_id: "one",
  question: "域名和地址有什么区别？",
  created_at: "2026-09-16",
  answer: null,
};

it("records not-understood once and requests another explanation on the same step", async () => {
  const user = userEvent.setup();
  const original = {
    ...pendingQuestion,
    answer: { kind: "comparison" as const, text: "第一种解释", card_id: null },
  };
  const retry = { ...pendingQuestion, id: "retry", doubt_id: "doubt" };
  const api = client(session({ questions: [original] }));
  vi.mocked(api.questionFeedback).mockResolvedValue(
    session({
      state: "ready",
      revision: 3,
      questions: [
        { ...original, feedback: "unresolved", doubt_id: "doubt" },
        retry,
      ],
    }),
  );
  vi.mocked(api.continue).mockResolvedValue(
    session({
      revision: 4,
      questions: [
        { ...original, feedback: "unresolved", doubt_id: "doubt" },
        {
          ...retry,
          answer: {
            kind: "example",
            text: "换一种具体情境来解释",
            card_id: null,
          },
        },
      ],
    }),
  );
  show(api);
  await user.dblClick(await screen.findByRole("button", { name: "还没懂" }));
  expect(api.questionFeedback).toHaveBeenCalledExactlyOnceWith(
    "study",
    pendingQuestion.id,
    "unresolved",
  );
  expect(api.continue).toHaveBeenCalledExactlyOnceWith("study");
  expect(await screen.findByText("换一种具体情境来解释")).toBeVisible();
  expect(screen.getByText(/第 1 步/)).toBeVisible();
  expect(api.feedback).not.toHaveBeenCalled();
});

it("records understood locally without grading or calling the model", async () => {
  const original = {
    ...pendingQuestion,
    answer: { kind: "example" as const, text: "一段解释", card_id: null },
  };
  const api = client(session({ questions: [original] }));
  vi.mocked(api.questionFeedback).mockResolvedValue(
    session({
      revision: 3,
      questions: [{ ...original, feedback: "understood" }],
    }),
  );
  show(api);
  await userEvent
    .setup()
    .click(await screen.findByRole("button", { name: "明白了" }));
  expect(
    await screen.findByText("你反馈已明白，不计为掌握成绩。"),
  ).toBeVisible();
  expect(api.continue).not.toHaveBeenCalled();
  expect(api.feedback).not.toHaveBeenCalled();
});

it("previews an unresolved question and starts its exact context only on request", async () => {
  const api = client();
  const pending = session({
    state: "ready",
    questions: [pendingQuestion],
    doubt_target: { id: "doubt", question: pendingQuestion.question },
  });
  vi.mocked(api.startDoubt).mockResolvedValue(pending);
  render(
    <StudyPanel
      topics={[]}
      providers={[provider]}
      client={api}
      doubtTarget={{
        id: "doubt",
        question: pendingQuestion.question,
        topic: "网络",
        session_id: "old",
      }}
      onExit={vi.fn()}
      onModelSettings={vi.fn()}
    />,
  );
  const preview = await screen.findByRole("region", { name: "继续疑问预览" });
  expect(within(preview).getByRole("heading")).toHaveTextContent(
    pendingQuestion.question,
  );
  expect(api.startDoubt).not.toHaveBeenCalled();
  expect(api.continue).not.toHaveBeenCalled();
  await userEvent
    .setup()
    .click(within(preview).getByRole("button", { name: "接着解决这个问题" }));
  expect(api.startDoubt).toHaveBeenCalledExactlyOnceWith("doubt");
  expect(api.continue).toHaveBeenCalledExactlyOnceWith("study");
});

it("saves the exact answer, links back to it, and removes only the highlight", async () => {
  const user = userEvent.setup();
  const answer = {
    kind: "explanation" as const,
    text: "域名是名称，地址用于定位。",
    card_id: null,
  };
  const api = client(session({ questions: [{ ...pendingQuestion, answer }] }));
  let notes: StudyHighlight[] = [];
  vi.mocked(api.highlights).mockImplementation(() => Promise.resolve(notes));
  vi.mocked(api.saveHighlight).mockImplementation((id, kind, sourceId) => {
    notes = [
      {
        id: "note",
        session_id: id,
        source_kind: kind,
        source_id: sourceId,
        title: pendingQuestion.question,
        text: answer.text,
        created_at: "2026-09-16",
      },
    ];
    return Promise.resolve("note");
  });
  vi.mocked(api.removeHighlight).mockImplementation(() => {
    notes = [];
    return Promise.resolve();
  });
  show(api);
  await user.dblClick(
    await screen.findByRole("button", { name: "保存这段回答" }),
  );
  expect(api.saveHighlight).toHaveBeenCalledExactlyOnceWith(
    "study",
    "question",
    pendingQuestion.id,
  );
  const saved = await screen.findByRole("region", { name: "学习收获" });
  expect(within(saved).getByText(answer.text)).toBeVisible();
  await user.click(within(saved).getByRole("button", { name: "查看原始问答" }));
  expect(
    document.getElementById(`study-source-${pendingQuestion.id}`),
  ).toHaveFocus();
  await user.click(within(saved).getByRole("button", { name: "移出学习收获" }));
  await waitFor(() =>
    expect(
      screen.queryByRole("region", { name: "学习收获" }),
    ).not.toBeInTheDocument(),
  );
  expect(screen.getByText(answer.text)).toBeVisible();
  expect(api.feedback).not.toHaveBeenCalled();
  expect(api.continue).not.toHaveBeenCalled();
});

it("opens the original question in a completed one-step session without generating again", async () => {
  const api = client(
    session({
      state: "completed",
      questions: [
        {
          ...pendingQuestion,
          answer: { kind: "example", text: "原始解释", card_id: null },
        },
      ],
    }),
  );
  render(
    <StudyPanel
      topics={[]}
      providers={[]}
      client={api}
      sessionId="study"
      focusSourceId={pendingQuestion.id}
      onExit={vi.fn()}
      onModelSettings={vi.fn()}
    />,
  );
  expect(await screen.findByText("原始解释")).toBeVisible();
  expect(
    document.getElementById(`study-source-${pendingQuestion.id}`),
  ).toHaveFocus();
  expect(api.continue).not.toHaveBeenCalled();
});

it("opens a saved older answer instead of focusing the latest answer", async () => {
  const api = client(
    session({
      questions: [
        {
          ...pendingQuestion,
          answer: { kind: "comparison", text: "保存的原始解释", card_id: null },
        },
        {
          ...pendingQuestion,
          id: "later-answer",
          answer: { kind: "example", text: "后来的解释", card_id: null },
        },
      ],
    }),
  );
  render(
    <StudyPanel
      topics={[]}
      providers={[provider]}
      client={api}
      sessionId="study"
      focusSourceId={pendingQuestion.id}
      onExit={vi.fn()}
      onModelSettings={vi.fn()}
    />,
  );
  await screen.findByText("后来的解释");
  expect(
    document.getElementById(`study-source-${pendingQuestion.id}`),
  ).toHaveFocus();
  expect(api.continue).not.toHaveBeenCalled();
});

it("answers a specific question once and stays on the current lesson without grading", async () => {
  const user = userEvent.setup();
  const api = client(session());
  vi.mocked(api.ask).mockImplementation((_id, _step, question, id) =>
    Promise.resolve(
      session({
        state: "ready",
        revision: 3,
        can_resume: true,
        can_ask: false,
        questions: [{ ...pendingQuestion, id, question }],
      }),
    ),
  );
  let reply!: (value: StudySession) => void;
  vi.mocked(api.continue).mockReturnValue(
    new Promise((resolve) => {
      reply = resolve;
    }),
  );
  show(api);
  await user.type(
    await screen.findByLabelText("我想问……"),
    pendingQuestion.question,
  );
  await user.dblClick(screen.getByRole("button", { name: "发送问题" }));
  expect(api.ask).toHaveBeenCalledTimes(1);
  expect(api.ask).toHaveBeenCalledWith(
    "study",
    "one",
    pendingQuestion.question,
    expect.any(String),
  );
  expect(screen.getByRole("button", { name: "继续" })).toBeDisabled();
  expect(api.feedback).not.toHaveBeenCalled();
  reply(
    session({
      revision: 5,
      questions: [
        {
          ...pendingQuestion,
          answer: {
            kind: "comparison",
            text: "名字可以不变，地址可以更新。",
            card_id: null,
          },
        },
      ],
    }),
  );
  expect(await screen.findByText("名字可以不变，地址可以更新。")).toBeVisible();
  expect(
    screen.getByText("名字可以不变，地址可以更新。").closest("section"),
  ).toHaveFocus();
  expect(screen.getByRole("heading", { name: "DNS 是什么" })).toBeVisible();
  expect(screen.getByRole("button", { name: "继续" })).toBeEnabled();
  expect(api.continue).toHaveBeenCalledTimes(1);
  expect(screen.getByLabelText("我想问……")).toHaveValue("");
});

it("keeps an unsaved question on error and reuses its request ID when retrying", async () => {
  const user = userEvent.setup();
  const api = client(session());
  vi.mocked(api.ask).mockRejectedValue(new Error("保存失败"));
  show(api);
  await user.type(
    await screen.findByLabelText("我想问……"),
    pendingQuestion.question,
  );
  await user.click(screen.getByRole("button", { name: "发送问题" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("保存失败");
  expect(screen.getByLabelText("我想问……")).toHaveValue(
    pendingQuestion.question,
  );
  await user.click(screen.getByRole("button", { name: "发送问题" }));
  expect(vi.mocked(api.ask).mock.calls[0]?.[3]).toBe(
    vi.mocked(api.ask).mock.calls[1]?.[3],
  );
  expect(api.continue).not.toHaveBeenCalled();
});

it("restores a saved unanswered question and retries the answer without resubmitting", async () => {
  const user = userEvent.setup();
  const api = client(
    session({
      state: "failed",
      can_resume: true,
      can_ask: false,
      questions: [pendingQuestion],
    }),
  );
  vi.mocked(api.continue).mockResolvedValue(
    session({
      revision: 4,
      questions: [
        {
          ...pendingQuestion,
          answer: {
            kind: "example",
            text: "像搬家后更新地址一样。",
            card_id: null,
          },
        },
      ],
    }),
  );
  show(api);
  expect(
    await screen.findByText(pendingQuestion.question, { exact: false }),
  ).toBeVisible();
  expect(api.continue).not.toHaveBeenCalled();
  await user.click(screen.getByRole("button", { name: "继续回答这个问题" }));
  expect(await screen.findByText("像搬家后更新地址一样。")).toBeVisible();
  expect(api.ask).not.toHaveBeenCalled();
  expect(api.continue).toHaveBeenCalledExactlyOnceWith("study");
});

it("waits for a consolidation question before offering local completion", async () => {
  const api = client(
    session({
      state: "ready",
      can_resume: true,
      can_ask: false,
      review_target: { concept_key: "dns", title: "DNS" },
      questions: [pendingQuestion],
      steps: [
        {
          ...session().steps[0]!,
          kind: "quiz",
          feedback: "answer",
          quiz: {
            options: ["地址", "文件"],
            selected: 0,
            correct_index: 0,
            correct: true,
            explanation: "查询地址。",
          },
        },
      ],
      summary: { topics: [], answered: 1, correct: 1 },
    }),
  );
  show(api);
  expect(
    await screen.findByRole("button", { name: "继续回答这个问题" }),
  ).toBeEnabled();
  expect(
    screen.queryByRole("button", { name: "完成巩固并查看小结" }),
  ).not.toBeInTheDocument();
  expect(screen.getByText("这次答对了")).toBeVisible();
  expect(api.feedback).not.toHaveBeenCalled();
});

const reviewTarget = {
  concept_key: "dns",
  topic: "网络",
  title: "DNS",
  attempts: 1,
  last_correct: false,
  due_at: 1,
};

it("previews the chosen card and sends only its exact ID after explicit start", async () => {
  const api = client(session({ state: "completed" }));
  const user = userEvent.setup();
  render(
    <StudyPanel
      topics={[]}
      providers={[provider]}
      onExit={vi.fn()}
      onModelSettings={vi.fn()}
      client={api}
      cardTarget={{
        id: "dns-card",
        question: "为什么需要 DNS？",
        topicLabel: "网络",
      }}
    />,
  );
  await screen.findByRole("region", { name: "卡片学习准备" });
  expect(api.startCard).not.toHaveBeenCalled();
  expect(api.continue).not.toHaveBeenCalled();
  await user.click(screen.getByRole("button", { name: "开始围绕这张卡学习" }));
  expect(api.startCard).toHaveBeenCalledExactlyOnceWith("dns-card", false);
  await waitFor(() =>
    expect(api.continue).toHaveBeenCalledExactlyOnceWith("study"),
  );
});

it("preserves an unfinished session when arriving from a different card", async () => {
  const saved = session({
    source_card: { card_id: "old-card", title: "上次的卡片" },
  });
  const api = client(saved);
  render(
    <StudyPanel
      topics={[]}
      providers={[provider]}
      onExit={vi.fn()}
      onModelSettings={vi.fn()}
      client={api}
      cardTarget={{ id: "new-card", question: "新的卡片", topicLabel: "网络" }}
    />,
  );
  await screen.findByText(/已找回上次未结束的学习/);
  expect(screen.getByText("本次围绕：上次的卡片")).toBeVisible();
  expect(
    screen.queryByRole("region", { name: "卡片学习准备" }),
  ).not.toBeInTheDocument();
  expect(api.startCard).not.toHaveBeenCalled();
  expect(api.continue).not.toHaveBeenCalled();
});
it("previews one consolidation target before starting any model request", async () => {
  const api = client();
  render(
    <StudyPanel
      topics={[]}
      providers={[provider]}
      onExit={vi.fn()}
      onModelSettings={vi.fn()}
      client={api}
      reviewTarget={reviewTarget}
    />,
  );
  await screen.findByRole("region", { name: "巩固准备" });
  expect(api.startReview).not.toHaveBeenCalled();
  expect(api.continue).not.toHaveBeenCalled();
  expect(screen.getByText(/会把这条练习的题目/)).toBeVisible();
  await userEvent.click(screen.getByRole("button", { name: "开始巩固" }));
  await screen.findByRole("heading", { name: "DNS 是什么" });
  expect(api.startReview).toHaveBeenCalledWith("dns");
  expect(api.continue).toHaveBeenCalledTimes(1);
  expect(api.start).not.toHaveBeenCalled();
});

it("restores a pending session if the review suggestion is stale", async () => {
  const api = client(session());
  render(
    <StudyPanel
      topics={[]}
      providers={[provider]}
      onExit={vi.fn()}
      onModelSettings={vi.fn()}
      client={api}
      reviewTarget={reviewTarget}
    />,
  );
  await screen.findByRole("heading", { name: "DNS 是什么" });
  expect(
    screen.queryByRole("button", { name: "开始巩固" }),
  ).not.toBeInTheDocument();
  expect(api.startReview).not.toHaveBeenCalled();
  expect(api.continue).not.toHaveBeenCalled();
});

it("explains that leaving a review without an answer preserves its schedule", async () => {
  const api = client(
    session({
      state: "completed",
      review_target: { concept_key: "dns", title: "DNS" },
    }),
  );
  show(api);
  expect(
    await screen.findByText("本次没有提交新答案，原来的巩固安排保留。"),
  ).toBeVisible();
});

it("can finish an answered review locally even when the model budget is exhausted", async () => {
  const completedQuiz = {
    ...session().steps[0]!,
    kind: "quiz" as const,
    feedback: "answer" as const,
    quiz: {
      options: ["地址", "文件"],
      selected: 0,
      correct_index: 0,
      correct: true,
      explanation: "DNS 查询地址。",
    },
  };
  const ready = session({
    state: "ready",
    can_resume: false,
    review_target: { concept_key: "dns", title: "DNS" },
    steps: [completedQuiz],
    summary: { topics: [], answered: 1, correct: 1 },
  });
  const api = client(ready);
  vi.mocked(api.continue).mockResolvedValue({ ...ready, state: "completed" });
  render(
    <StudyPanel
      topics={[]}
      providers={[]}
      onExit={vi.fn()}
      onModelSettings={vi.fn()}
      client={api}
    />,
  );
  await userEvent.click(
    await screen.findByRole("button", { name: "完成巩固并查看小结" }),
  );
  expect(await screen.findByText("已按这次作答安排下次回顾。")).toBeVisible();
  expect(api.continue).toHaveBeenCalledTimes(1);
});

it("starts from a short goal and sends feedback to the next step", async () => {
  const user = userEvent.setup();
  const api = client();
  show(api);
  await user.type(await screen.findByLabelText("今天想了解什么？"), "理解 DNS");
  await user.click(screen.getByRole("button", { name: "开始学习" }));
  expect(
    await screen.findByRole("heading", { name: "DNS 是什么" }),
  ).toBeVisible();
  expect(api.start).toHaveBeenCalledWith("理解 DNS", "理解 DNS");
  vi.mocked(api.continue).mockResolvedValue(
    session({
      revision: 4,
      steps: [
        session().steps[0]!,
        {
          ...session().steps[0]!,
          id: "two",
          kind: "example",
          title: "像查通讯录一样",
          text: "通过名字查询地址。",
        },
      ],
    }),
  );
  await user.click(screen.getByRole("button", { name: "没看懂" }));
  expect(api.feedback).toHaveBeenCalledWith(
    "study",
    "one",
    "confused",
    undefined,
  );
  expect(
    await screen.findByRole("heading", { name: "像查通讯录一样", level: 2 }),
  ).toBeVisible();
});

it("restores saved content without requesting another model response", async () => {
  const api = client(session());
  show(api);
  expect(
    await screen.findByRole("heading", { name: "DNS 是什么" }),
  ).toBeVisible();
  expect(api.start).not.toHaveBeenCalled();
  expect(api.continue).not.toHaveBeenCalled();
});

it("opens the exact saved session selected from the home screen without generating", async () => {
  const api = client(session());
  render(
    <StudyPanel
      topics={[]}
      providers={[provider]}
      onExit={vi.fn()}
      onModelSettings={vi.fn()}
      client={api}
      sessionId="saved-id"
    />,
  );
  await screen.findByRole("heading", { name: "DNS 是什么" });
  expect(api.read).toHaveBeenCalledWith("saved-id");
  expect(api.latest).not.toHaveBeenCalled();
  expect(api.continue).not.toHaveBeenCalled();
  expect(api.start).not.toHaveBeenCalled();
});

it("hides quiz answers until submission and waits before the next model request", async () => {
  const user = userEvent.setup();
  const quiz = {
    ...session().steps[0]!,
    kind: "quiz" as const,
    text: "DNS 的用途是什么？",
    quiz: {
      options: ["查找地址", "压缩文件"],
      selected: null,
      correct: null,
      correct_index: null,
      explanation: null,
    },
  };
  const api = client(session({ steps: [quiz] }));
  vi.mocked(api.feedback).mockResolvedValue(
    session({
      state: "ready",
      revision: 3,
      can_resume: true,
      summary: { topics: [], answered: 1, correct: 0 },
      steps: [
        {
          ...quiz,
          feedback: "answer",
          quiz: {
            ...quiz.quiz,
            selected: 1,
            correct: false,
            correct_index: 0,
            explanation: "域名查询与压缩文件是不同的事。",
          },
        },
      ],
    }),
  );
  show(api);
  const submit = await screen.findByRole("button", { name: "提交答案" });
  expect(submit).toBeDisabled();
  expect(
    screen.queryByText("域名查询与压缩文件是不同的事。"),
  ).not.toBeInTheDocument();
  await user.click(screen.getByRole("radio", { name: "压缩文件" }));
  await user.click(submit);
  expect(
    await screen.findByText("域名查询与压缩文件是不同的事。"),
  ).toBeVisible();
  expect(api.feedback).toHaveBeenCalledWith("study", "one", "answer", 1);
  expect(api.continue).not.toHaveBeenCalled();
  expect(screen.getByRole("button", { name: "继续上次学习" })).toBeEnabled();
});

it("keeps completed summary when a late model response arrives after ending", async () => {
  const user = userEvent.setup();
  const api = client(session({ state: "paused", can_resume: true }));
  let resolve!: (run: StudySession) => void;
  vi.mocked(api.continue).mockReturnValue(
    new Promise((done) => {
      resolve = done;
    }),
  );
  vi.mocked(api.pause).mockResolvedValue(
    session({
      state: "completed",
      revision: 9,
      next_topic: "DNS 缓存",
      can_resume: false,
    }),
  );
  show(api);
  await user.click(await screen.findByRole("button", { name: "继续上次学习" }));
  await user.click(screen.getByRole("button", { name: "结束并查看小结" }));
  expect(await screen.findByRole("region", { name: "学习小结" })).toBeVisible();
  resolve(session({ revision: 8 }));
  await waitFor(() =>
    expect(screen.getByRole("region", { name: "学习小结" })).toBeVisible(),
  );
  expect(
    screen.queryByRole("button", { name: "没看懂" }),
  ).not.toBeInTheDocument();
});

it("preserves progress after failure, avoids duplicate clicks, and can leave", async () => {
  const user = userEvent.setup();
  const api = client(session({ state: "paused", can_resume: true }));
  const exit = vi.fn();
  let reject!: (e: Error) => void;
  vi.mocked(api.continue).mockReturnValue(
    new Promise((_, fail) => {
      reject = fail;
    }),
  );
  show(api, exit);
  await user.dblClick(
    await screen.findByRole("button", { name: "继续上次学习" }),
  );
  expect(api.continue).toHaveBeenCalledTimes(1);
  reject(new Error("网络中断"));
  expect(await screen.findByRole("alert")).toHaveTextContent("网络中断");
  expect(screen.getByRole("heading", { name: "DNS 是什么" })).toBeVisible();
  await user.click(screen.getByRole("button", { name: "稍后继续" }));
  expect(api.pause).toHaveBeenCalledWith("study", false);
  expect(exit).toHaveBeenCalled();
});

it("shows an honest summary when no practice was submitted", async () => {
  const api = client(session({ state: "completed", next_topic: "DNS 缓存" }));
  show(api);
  const summary = await screen.findByRole("region", { name: "学习小结" });
  expect(
    within(summary).getByText("本次没有提交练习答案，未评估掌握程度。"),
  ).toBeVisible();
});

it("routes unconfigured users to model settings without calling generation", async () => {
  const user = userEvent.setup();
  const api = client();
  const configure = vi.fn();
  render(
    <StudyPanel
      topics={[]}
      providers={[]}
      onExit={vi.fn()}
      onModelSettings={configure}
      client={api}
    />,
  );
  await user.type(await screen.findByLabelText("今天想了解什么？"), "DNS");
  expect(screen.getByRole("button", { name: "开始学习" })).toBeDisabled();
  await user.click(screen.getByRole("button", { name: "配置学习模型 →" }));
  expect(configure).toHaveBeenCalled();
  expect(api.start).not.toHaveBeenCalled();
});

it("requires confirmation to reset learning records and returns to the start form", async () => {
  const user = userEvent.setup();
  const api = client(session({ state: "completed" }));
  show(api);
  await user.click(await screen.findByText("本机学习记录"));
  await user.click(screen.getByRole("button", { name: "清除陪伴学习记录" }));
  expect(api.reset).not.toHaveBeenCalled();
  const dialog = screen.getByRole("dialog", { name: "清除陪伴学习记录？" });
  await user.click(within(dialog).getByRole("button", { name: "确认清除" }));
  expect(await screen.findByLabelText("今天想了解什么？")).toBeVisible();
  expect(api.reset).toHaveBeenCalledOnce();
});
