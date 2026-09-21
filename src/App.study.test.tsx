import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { vi } from "vitest";
import App from "./App";
import * as api from "./lib/api";
import { studyClient, type StudySession } from "./lib/study";
const openWindow = vi.fn<typeof window.open>();

beforeEach(async () => {
  await api.clearData("all");
  const data = await api.bootstrapApp({ recordShown: false });
  vi.spyOn(api, "bootstrapApp").mockResolvedValue({
    ...data,
    onboardingComplete: true,
  });
  vi.spyOn(studyClient, "latest").mockResolvedValue(null);
  vi.spyOn(studyClient, "history").mockResolvedValue([]);
  openWindow.mockReset().mockReturnValue(null);
  vi.spyOn(window, "open").mockImplementation(openWindow);
});
afterEach(() => vi.restoreAllMocks());

it("switches to study in the current window, opens model settings, and returns to generation", async () => {
  const user = userEvent.setup();
  render(<App />);
  await user.click(await screen.findByRole("button", { name: "打开菜单" }));
  await user.click(screen.getByRole("button", { name: "学习中心" }));
  expect(
    await screen.findByRole("heading", { name: "接着学一点" }),
  ).toBeVisible();
  expect(
    screen.queryByRole("navigation", { name: "主要页面" }),
  ).not.toBeInTheDocument();
  expect(screen.getByRole("button", { name: "打开学习中心" })).toHaveAttribute(
    "aria-current",
    "page",
  );
  expect(
    screen.queryByRole("heading", { name: "想探索哪个领域？" }),
  ).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: /陪我学一会儿/ }));
  await user.type(
    await screen.findByRole("textbox", { name: "今天想了解什么？" }),
    "理解网络",
  );
  await user.click(screen.getByRole("button", { name: "打开学习中心" }));
  expect(screen.getByRole("textbox", { name: "今天想了解什么？" })).toHaveValue(
    "理解网络",
  );
  await user.click(screen.getByRole("button", { name: "打开菜单" }));
  expect(screen.getByRole("button", { name: "学习中心" })).toHaveAttribute(
    "aria-current",
    "page",
  );
  await user.click(screen.getByRole("button", { name: "学习中心" }));
  expect(screen.getByRole("textbox", { name: "今天想了解什么？" })).toHaveValue(
    "理解网络",
  );
  await user.click(screen.getByRole("button", { name: "配置学习模型 →" }));
  expect(
    await screen.findByRole("heading", { name: "模型设置" }),
  ).toBeVisible();
  await user.click(screen.getByRole("button", { name: "打开学习中心" }));
  await user.click(
    await screen.findByRole("button", { name: "← 返回知识小窗" }),
  );
  expect(
    await screen.findByRole("heading", { name: "想探索哪个领域？" }),
  ).toBeVisible();
  expect(
    screen.queryByRole("region", { name: "我的短学习" }),
  ).not.toBeInTheDocument();
  expect(openWindow).not.toHaveBeenCalled();
});

it("opens a card's saved study in the same page and returns to the original revealed card", async () => {
  const user = userEvent.setup();
  const data = await api.bootstrapApp({ recordShown: false });
  const card = data.card!;
  const saved: StudySession = {
    id: "saved",
    goal: card.question,
    topic: card.topicLabel,
    provider: "deepseek",
    model: "test",
    state: "completed",
    revision: 1,
    created_at: "2026-09-21",
    steps: [],
    questions: [],
    error: null,
    next_topic: null,
    can_resume: false,
    review_target: null,
    source_card: { card_id: card.id, title: card.question },
    source_expanded: true,
    can_ask: false,
    question_limit: 4,
    summary: { topics: [], answered: 0, correct: 0 },
  };
  vi.spyOn(studyClient, "cardSessions").mockResolvedValue([
    {
      id: saved.id,
      goal: saved.goal,
      state: saved.state,
      created_at: saved.created_at,
      step_count: 1,
    },
  ]);
  const read = vi.spyOn(studyClient, "read").mockResolvedValue(saved);
  vi.spyOn(studyClient, "sourceCard").mockResolvedValue(card);
  render(<App />);
  await user.click(
    await screen.findByRole("button", { name: /浏览现有知识点/ }),
  );
  await user.click(screen.getByRole("button", { name: "我想好了，揭晓答案" }));
  await user.click(await screen.findByRole("button", { name: /回看上次学习/ }));
  await waitFor(() => expect(read).toHaveBeenCalledWith("saved"));
  await user.click(await screen.findByRole("button", { name: "回到原卡" }));
  expect(
    await screen.findByRole("heading", { name: card.question }),
  ).toBeVisible();
  expect(screen.getByText("简短答案")).toBeVisible();
  expect(screen.getByRole("button", { name: "收起详细解释" })).toBeVisible();
  expect(openWindow).not.toHaveBeenCalled();
});
