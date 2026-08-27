import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { vi } from "vitest";
import { fallbackCards } from "../data/fallbackCards";
import { KnowledgeCardView } from "./KnowledgeCardView";

describe("KnowledgeCardView", () => {
  const baseHandlers = {
    onInteraction: vi.fn().mockResolvedValue(undefined),
    onReturnHome: vi.fn(),
    onPrevious: vi.fn(),
    onNext: vi.fn().mockResolvedValue(undefined),
    onDismiss: vi.fn().mockResolvedValue(undefined),
    onMaster: vi.fn().mockResolvedValue(undefined),
    onGenerateSameTopic: vi.fn().mockResolvedValue(undefined),
    onGenerateRandomTopic: vi.fn().mockResolvedValue(undefined),
  };

  beforeEach(() => {
    Object.values(baseHandlers).forEach((handler) => handler.mockClear());
  });

  it("shows the new main actions, then replaces them with answer actions", async () => {
    const user = userEvent.setup();
    render(
      <KnowledgeCardView
        card={fallbackCards[0]!}
        availableCardCount={12}
        busy={false}
        canGoPrevious={true}
        {...baseHandlers}
      />,
    );

    expect(
      screen.queryByText(`${fallbackCards[0]!.estimatedReadSeconds} 秒`),
    ).not.toBeInTheDocument();
    expect(
      screen.getByRole("status", {
        name: "当前可展示 12 张知识卡",
      }),
    ).toHaveTextContent("可展示 12 张");
    await user.click(screen.getByRole("button", { name: "返回主界面" }));
    expect(baseHandlers.onReturnHome).toHaveBeenCalledOnce();

    const dismiss = screen.getByRole("button", { name: "不感兴趣" });
    const favorite = screen.getByRole("button", { name: "收藏" });
    expect(dismiss.closest(".footer-center")).toBe(
      favorite.closest(".footer-center"),
    );

    expect(screen.queryByText("简短答案")).not.toBeInTheDocument();
    await user.click(
      screen.getByRole("button", { name: "我想好了，揭晓答案" }),
    );
    expect(screen.getByText("简短答案")).toBeVisible();
    expect(baseHandlers.onInteraction).toHaveBeenCalledWith("revealed");
    expect(
      screen.queryByRole("button", { name: "不感兴趣" }),
    ).not.toBeInTheDocument();
    for (const removed of ["喜欢", "已知道", "内容有误"]) {
      expect(
        screen.queryByRole("button", { name: removed }),
      ).not.toBeInTheDocument();
    }

    await user.click(screen.getByRole("button", { name: /展开详细解释/ }));
    expect(screen.getByRole("region", { name: "详细解释" })).toBeVisible();
    expect(baseHandlers.onInteraction).toHaveBeenCalledWith("expanded");

    await user.click(
      screen.getByRole("button", {
        name: "再次生成随机领域知识点",
      }),
    );
    expect(baseHandlers.onGenerateRandomTopic).toHaveBeenCalledOnce();

    await user.click(screen.getByRole("button", { name: "已狠狠涨知识" }));
    expect(baseHandlers.onMaster).toHaveBeenCalledOnce();

    await user.click(
      screen.getByRole("button", { name: "再次生成同领域知识点" }),
    );
    expect(baseHandlers.onGenerateSameTopic).toHaveBeenCalledOnce();

    await user.click(screen.getByRole("button", { name: "返回" }));
    expect(screen.queryByText("简短答案")).not.toBeInTheDocument();
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent(
      fallbackCards[0]!.question,
    );
  });

  it("supports previous, dismiss, favorite, and next on the main card", async () => {
    const user = userEvent.setup();
    render(
      <KnowledgeCardView
        card={fallbackCards[1]!}
        availableCardCount={1}
        busy={false}
        canGoPrevious={false}
        {...baseHandlers}
      />,
    );

    await user.click(screen.getByRole("button", { name: "上一条" }));
    expect(baseHandlers.onPrevious).toHaveBeenCalledOnce();
    await user.click(screen.getByRole("button", { name: "不感兴趣" }));
    expect(baseHandlers.onDismiss).toHaveBeenCalledOnce();
    await user.click(screen.getByRole("button", { name: "收藏" }));
    expect(baseHandlers.onInteraction).toHaveBeenCalledWith("favorited");
    await user.click(screen.getByRole("button", { name: /下一条/ }));
    expect(baseHandlers.onNext).toHaveBeenCalledOnce();
    expect(
      screen.queryByRole("button", { name: "生成随机领域" }),
    ).not.toBeInTheDocument();
  });
});
