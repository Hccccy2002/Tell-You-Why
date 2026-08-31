import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import App from "./App";
import { fallbackCards } from "./data/fallbackCards";
import { clearData } from "./lib/api";

async function markCurrentCardNotInterested(
  user: ReturnType<typeof userEvent.setup>,
) {
  await user.click(screen.getByRole("button", { name: "不感兴趣" }));
  const dialog = screen.getByRole("dialog", {
    name: "将这张知识卡标记为不感兴趣？",
  });
  await user.click(
    within(dialog).getByRole("button", { name: "标记为不感兴趣" }),
  );
}

describe("critical local user flow", () => {
  beforeEach(async () => {
    await clearData("all");
  });

  it("supports reveal return, no-key prompt, previous, and confirmed dismiss", async () => {
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
    await user.click(screen.getByRole("button", { name: /去配置/ }));
    expect(
      await screen.findByRole("heading", { name: "模型设置" }),
    ).toBeVisible();
    await user.keyboard("{Escape}");
    expect(
      await screen.findByRole("heading", { name: "想探索哪个领域？" }),
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
    expect(
      screen.queryByRole("button", { name: "去配置" }),
    ).not.toBeInTheDocument();
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

    await markCurrentCardNotInterested(user);
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
      await markCurrentCardNotInterested(user);
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
  }, 10_000);

  it("shows a hidden history card in favorites after it is favorited", async () => {
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
    await user.click(screen.getByRole("button", { name: /浏览现有知识点/ }));

    const hiddenQuestion =
      screen.getByRole("heading", { level: 1 }).textContent ?? "";
    expect(hiddenQuestion).not.toBe("");

    await markCurrentCardNotInterested(user);
    await waitFor(() => {
      expect(screen.getByRole("heading", { level: 1 })).not.toHaveTextContent(
        hiddenQuestion,
      );
    });

    await user.click(screen.getByRole("button", { name: "打开菜单" }));
    await user.click(screen.getByRole("button", { name: "收藏与历史" }));
    await user.click(screen.getByRole("button", { name: "最近浏览" }));
    await user.click(await screen.findByText(hiddenQuestion));

    await user.click(screen.getByRole("button", { name: "收藏" }));
    expect(
      await screen.findByRole("button", { name: "已收藏" }),
    ).toHaveAttribute("aria-pressed", "true");

    await user.click(screen.getByRole("button", { name: "打开菜单" }));
    await user.click(screen.getByRole("button", { name: "收藏与历史" }));

    const favoriteEntry = await screen.findByText(hiddenQuestion);
    expect(favoriteEntry).toBeVisible();
    await user.click(favoriteEntry);
    expect(
      await screen.findByRole("button", { name: "已收藏" }),
    ).toHaveAttribute("aria-pressed", "true");
  });

  it("drops session navigation history after clearing recent history from the library", async () => {
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
    await user.click(screen.getByRole("button", { name: /浏览现有知识点/ }));

    const firstQuestion = screen.getByRole("heading", {
      level: 1,
    }).textContent;
    await user.click(screen.getByRole("button", { name: /下一条/ }));
    await waitFor(() => {
      expect(screen.getByRole("heading", { level: 1 }).textContent).not.toBe(
        firstQuestion,
      );
    });
    expect(screen.getByRole("button", { name: "上一条" })).toBeEnabled();

    await user.click(screen.getByRole("button", { name: "打开菜单" }));
    await user.click(screen.getByRole("button", { name: "收藏与历史" }));
    await user.click(screen.getByRole("button", { name: "最近浏览" }));
    await screen.findByText(firstQuestion ?? "");
    await user.click(screen.getByRole("button", { name: "清除阅读记录" }));
    const dialog = screen.getByRole("dialog", {
      name: "清除所有阅读记录？",
    });
    await user.click(
      within(dialog).getByRole("button", { name: "清除阅读记录" }),
    );
    expect(await screen.findByText("还没有浏览记录")).toBeVisible();

    await user.click(screen.getByRole("button", { name: "打开菜单" }));
    await user.click(screen.getByRole("button", { name: "知识小窗" }));
    await user.click(screen.getByRole("button", { name: /浏览现有知识点/ }));
    expect(screen.getByRole("button", { name: "上一条" })).toBeDisabled();
  });
});
