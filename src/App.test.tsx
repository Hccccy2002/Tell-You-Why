import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import App from "./App";
import { fallbackCards } from "./data/fallbackCards";

describe("critical local user flow", () => {
  it("supports reveal return, no-key prompt, previous, and immediate dismiss", async () => {
    const user = userEvent.setup();
    render(<App />);

    await user.click(
      await screen.findByRole("button", { name: "选择我的兴趣" }),
    );
    await user.click(screen.getByRole("button", { name: "自然科学" }));
    await user.click(screen.getByRole("button", { name: "历史与文明" }));
    await user.click(screen.getByRole("button", { name: "计算机与互联网" }));
    await user.click(screen.getByRole("button", { name: "继续" }));
    await user.click(screen.getByRole("button", { name: "开始探索" }));

    expect(
      screen.getByRole("heading", { name: "想探索哪个领域？" }),
    ).toBeVisible();
    await user.click(screen.getByRole("button", { name: /浏览现有知识点/ }));
    expect(
      screen.getByRole("status", {
        name: `当前可展示 ${fallbackCards.length} 张知识卡`,
      }),
    ).toHaveTextContent(`可展示 ${fallbackCards.length} 张`);

    await user.click(screen.getByRole("button", { name: "返回主界面" }));
    expect(
      screen.getByRole("heading", { name: "想探索哪个领域？" }),
    ).toBeVisible();
    await user.click(screen.getByRole("button", { name: /浏览现有知识点/ }));

    const reveal = await screen.findByRole("button", {
      name: "我想好了，揭晓答案",
    });
    const firstQuestion = screen.getByRole("heading", { level: 1 }).textContent;
    await user.click(reveal);
    expect(screen.getByText("简短答案")).toBeVisible();
    await user.click(
      screen.getByRole("button", {
        name: "再次生成随机领域知识点",
      }),
    );
    expect(screen.getByText("请先配置模型哦~")).toHaveClass("floating-notice");
    await waitFor(
      () => {
        expect(screen.queryByText("请先配置模型哦~")).not.toBeInTheDocument();
      },
      { timeout: 1600 },
    );
    await user.click(screen.getByRole("button", { name: /展开详细解释/ }));
    expect(screen.getByRole("region", { name: "详细解释" })).toBeVisible();

    await user.click(
      screen.getByRole("button", { name: "再次生成同领域知识点" }),
    );
    expect(screen.getByText("请先配置模型哦~")).toHaveClass("floating-notice");
    await waitFor(
      () => {
        expect(screen.queryByText("请先配置模型哦~")).not.toBeInTheDocument();
      },
      { timeout: 1600 },
    );

    await user.click(screen.getByRole("button", { name: "返回" }));
    await user.click(screen.getByRole("button", { name: /下一条/ }));

    await waitFor(() => {
      expect(screen.getByRole("heading", { level: 1 }).textContent).not.toBe(
        firstQuestion,
      );
    });

    await user.click(screen.getByRole("button", { name: "上一条" }));
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent(
      firstQuestion ?? "",
    );

    await user.click(screen.getByRole("button", { name: "不感兴趣" }));
    await waitFor(() => {
      expect(screen.getByRole("heading", { level: 1 }).textContent).not.toBe(
        firstQuestion,
      );
      expect(
        screen.getByRole("status", {
          name: `当前可展示 ${fallbackCards.length - 1} 张知识卡`,
        }),
      ).toBeVisible();
    });
    expect(screen.getByRole("button", { name: "上一条" })).toBeDisabled();

    for (let index = 0; index < fallbackCards.length - 2; index += 1) {
      await user.click(screen.getByRole("button", { name: "不感兴趣" }));
      await waitFor(() => {
        expect(
          screen.getByRole("status", {
            name: `当前可展示 ${fallbackCards.length - index - 2} 张知识卡`,
          }),
        ).toBeVisible();
      });
    }

    await user.click(screen.getByRole("button", { name: /下一条/ }));
    expect(screen.getByText("当前没有知识点啦，快去生成吧~")).toHaveClass(
      "floating-notice",
    );
    await waitFor(
      () => {
        expect(
          screen.queryByText("当前没有知识点啦，快去生成吧~"),
        ).not.toBeInTheDocument();
      },
      { timeout: 1600 },
    );
    await user.click(screen.getByRole("button", { name: "上一条" }));
    expect(screen.getByText("当前没有知识点啦，快去生成吧~")).toHaveClass(
      "floating-notice",
    );
    expect(screen.getByRole("heading", { level: 1 })).not.toHaveTextContent(
      firstQuestion ?? "",
    );
  });
});
