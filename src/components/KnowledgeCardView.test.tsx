import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { vi } from "vitest";
import { fallbackCards } from "../data/fallbackCards";
import type { FollowUpResult } from "../types";
import { KnowledgeCardView } from "./KnowledgeCardView";

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((promiseResolve, promiseReject) => {
    resolve = promiseResolve;
    reject = promiseReject;
  });
  return { promise, reject, resolve };
}

describe("KnowledgeCardView", () => {
  const baseHandlers = {
    onAskFollowUp: vi.fn().mockResolvedValue({
      answer: "这是一个便于理解的补充回答。",
      providerId: "deepseek",
      model: "deepseek-v4-flash",
      switchedFromProviderId: null,
    }),
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

  it("asks follow-up questions with Enter and prior history", async () => {
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
      screen.queryByRole("textbox", { name: "输入追问" }),
    ).not.toBeInTheDocument();
    await user.click(
      screen.getByRole("button", { name: "我想好了，揭晓答案" }),
    );

    const input = screen.getByRole("textbox", { name: "输入追问" });
    for (const removed of [
      "能举个例子吗？",
      "为什么会这样？",
      "这和日常生活有什么关系？",
    ]) {
      expect(
        screen.queryByRole("button", { name: removed }),
      ).not.toBeInTheDocument();
    }
    await user.type(input, "能举个例子吗？");
    await user.click(screen.getByRole("button", { name: "发送" }));

    expect(
      await screen.findByText("这是一个便于理解的补充回答。"),
    ).toBeVisible();
    expect(baseHandlers.onAskFollowUp).toHaveBeenNthCalledWith(
      1,
      "能举个例子吗？",
      [],
    );
    expect(
      screen.getByText("DeepSeek · deepseek-v4-flash · AI 未核验"),
    ).toBeVisible();
    const userMessage = screen.getByRole("article", { name: "你的问题" });
    expect(userMessage).toHaveTextContent("能举个例子吗？");
    expect(userMessage.querySelector("strong")).toBeNull();

    await user.type(input, "能再解释一步吗？");
    await user.keyboard("{Enter}");
    await waitFor(() => {
      expect(baseHandlers.onAskFollowUp).toHaveBeenNthCalledWith(
        2,
        "能再解释一步吗？",
        [
          { role: "user", content: "能举个例子吗？" },
          {
            role: "assistant",
            content: "这是一个便于理解的补充回答。",
          },
        ],
      );
    });
  });

  it("renders AI Markdown formatting without executing raw HTML or unsafe links", async () => {
    const user = userEvent.setup();
    baseHandlers.onAskFollowUp.mockResolvedValueOnce({
      answer: [
        "## 核心结论",
        "第一行",
        "第二行",
        "",
        "这是 **重点**，也有 *说明* 和 ~~旧说法~~。",
        "",
        "- 第一项",
        "- 第二项",
        "",
        "`x = 1`",
        "",
        "```js",
        "const x = 1;",
        "```",
        "",
        "[安全链接](https://example.com)",
        "[不安全链接](javascript:alert(1))",
        "![远程图片](https://example.com/tracker.png)",
        "<script>window.__unsafe = true</script>",
      ].join("\n"),
      providerId: "deepseek",
      model: "deepseek-v4-flash",
      switchedFromProviderId: null,
    });
    const { container } = render(
      <KnowledgeCardView
        card={fallbackCards[0]!}
        availableCardCount={12}
        busy={false}
        canGoPrevious={true}
        {...baseHandlers}
      />,
    );

    await user.click(
      screen.getByRole("button", { name: "我想好了，揭晓答案" }),
    );
    await user.type(
      screen.getByRole("textbox", { name: "输入追问" }),
      "请用 Markdown 回答",
    );
    await user.click(screen.getByRole("button", { name: "发送" }));

    const markdown = await screen.findByText("核心结论");
    const markdownRoot = markdown.closest(".follow-up-markdown");
    expect(markdown).toHaveRole("heading");
    expect(markdownRoot).not.toBeNull();
    expect(screen.getByText("重点").tagName).toBe("STRONG");
    expect(screen.getByText("说明").tagName).toBe("EM");
    expect(screen.getByText("旧说法").tagName).toBe("DEL");
    expect(screen.getByRole("list")).toBeVisible();
    expect(screen.getAllByRole("listitem")).toHaveLength(2);
    expect(markdownRoot?.querySelector("br")).not.toBeNull();
    expect(markdownRoot?.querySelector("code")).toHaveTextContent("x = 1");
    expect(markdownRoot?.querySelector("pre code")).toHaveTextContent(
      "const x = 1;",
    );
    expect(
      screen.getByRole("button", { name: /安全链接/ }),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: /不安全链接/ }),
    ).not.toBeInTheDocument();
    expect(screen.getByText("不安全链接")).toBeVisible();
    expect(screen.getByText("远程图片")).toBeVisible();
    expect(container.querySelector("img")).toBeNull();
    expect(container.querySelector("script")).toBeNull();
    expect(markdownRoot).not.toHaveTextContent("**");
  });

  it("keeps Shift+Enter as a newline and ignores IME confirmation Enter", async () => {
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
    await user.click(
      screen.getByRole("button", { name: "我想好了，揭晓答案" }),
    );
    const input = screen.getByRole("textbox", { name: "输入追问" });

    await user.type(input, "第一行");
    fireEvent.keyDown(input, { key: "Enter", shiftKey: true });
    fireEvent.change(input, { target: { value: "第一行\n第二行" } });
    fireEvent.keyDown(input, { isComposing: true, key: "Enter" });
    fireEvent.keyDown(input, { key: "Enter", keyCode: 229 });
    expect(baseHandlers.onAskFollowUp).not.toHaveBeenCalled();

    fireEvent.keyDown(input, { key: "Enter" });
    await waitFor(() => {
      expect(baseHandlers.onAskFollowUp).toHaveBeenCalledWith(
        "第一行\n第二行",
        [],
      );
    });
  });

  it("deduplicates loading submissions and keeps the draft after an error", async () => {
    const user = userEvent.setup();
    const pending = deferred<FollowUpResult>();
    baseHandlers.onAskFollowUp.mockReturnValueOnce(pending.promise);
    render(
      <KnowledgeCardView
        card={fallbackCards[0]!}
        availableCardCount={12}
        busy={false}
        canGoPrevious={true}
        {...baseHandlers}
      />,
    );
    await user.click(
      screen.getByRole("button", { name: "我想好了，揭晓答案" }),
    );
    const input = screen.getByRole("textbox", { name: "输入追问" });
    await user.type(input, "请补充说明");
    await user.click(screen.getByRole("button", { name: "发送" }));

    expect(screen.getByRole("button", { name: "正在回答…" })).toBeDisabled();
    expect(input).toBeDisabled();
    const pendingUserMessage = screen.getByRole("article", {
      name: "你的问题",
    });
    expect(pendingUserMessage).toHaveTextContent("请补充说明");
    expect(pendingUserMessage.querySelector("strong")).toBeNull();
    const form = input.closest("form");
    expect(form).not.toBeNull();
    fireEvent.submit(form!);
    expect(baseHandlers.onAskFollowUp).toHaveBeenCalledOnce();

    await act(() => {
      pending.reject(new Error("模型服务暂时不可用"));
      return pending.promise.catch(() => undefined);
    });
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "模型服务暂时不可用",
    );
    expect(input).toHaveValue("请补充说明");
    expect(screen.getByRole("button", { name: "发送" })).toBeEnabled();

    await user.type(input, "，请重试");
    await user.click(screen.getByRole("button", { name: "发送" }));
    expect(
      await screen.findByText("这是一个便于理解的补充回答。"),
    ).toBeVisible();
  });

  it("preserves the local follow-up thread when returning to the question", async () => {
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
    await user.click(
      screen.getByRole("button", { name: "我想好了，揭晓答案" }),
    );
    await user.type(
      screen.getByRole("textbox", { name: "输入追问" }),
      "这个回答会被清空吗？",
    );
    await user.click(screen.getByRole("button", { name: "发送" }));
    expect(await screen.findByRole("log", { name: "追问对话" })).toBeVisible();
    await user.type(
      screen.getByRole("textbox", { name: "输入追问" }),
      "尚未发送的草稿",
    );

    await user.click(screen.getByRole("button", { name: "返回" }));
    await user.click(
      screen.getByRole("button", { name: "我想好了，揭晓答案" }),
    );
    expect(screen.getByRole("log", { name: "追问对话" })).toBeVisible();
    expect(screen.getByRole("textbox", { name: "输入追问" })).toHaveValue(
      "尚未发送的草稿",
    );
  });
});
