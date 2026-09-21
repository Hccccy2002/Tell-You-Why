import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { vi } from "vitest";
import type { FollowUpMessage, FollowUpResult } from "./types";

const { askFollowUpMock, listCardFollowUpsMock } = vi.hoisted(() => ({
  askFollowUpMock: vi.fn(),
  listCardFollowUpsMock: vi.fn(),
}));

vi.mock("./lib/api", async (importOriginal) => {
  const actual = await importOriginal();
  if (!actual || typeof actual !== "object") {
    throw new Error("Failed to load the real API module for this test");
  }
  return {
    ...actual,
    askFollowUp: askFollowUpMock,
    listCardFollowUps: listCardFollowUpsMock,
  };
});

import App from "./App";
import { clearData } from "./lib/api";

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((promiseResolve) => {
    resolve = promiseResolve;
  });
  return { promise, resolve };
}

async function openFirstLocalCard() {
  const user = userEvent.setup();
  await user.click(await screen.findByRole("button", { name: "选择我的兴趣" }));
  await user.click(screen.getByRole("button", { name: "自然科学" }));
  await user.click(screen.getByRole("button", { name: "历史与文明" }));
  await user.click(screen.getByRole("button", { name: "计算机与互联网" }));
  await user.click(screen.getByRole("button", { name: "继续" }));
  await user.click(screen.getByRole("button", { name: "开始探索" }));
  await user.click(screen.getByRole("button", { name: /浏览现有知识点/ }));
  return user;
}

describe("follow-up navigation lock", () => {
  beforeEach(async () => {
    askFollowUpMock.mockReset();
    listCardFollowUpsMock.mockReset().mockResolvedValue([]);
    await clearData("all");
  });

  it("locks all navigation until the pending AI answer is retained", async () => {
    const pending = deferred<FollowUpResult>();
    askFollowUpMock.mockReturnValueOnce(pending.promise);
    render(<App />);
    const user = await openFirstLocalCard();

    const questionHeading = screen.getByRole("heading", { level: 1 });
    const question = questionHeading.textContent;
    await user.click(
      screen.getByRole("button", { name: "我想好了，揭晓答案" }),
    );
    await user.type(
      screen.getByRole("textbox", { name: "输入追问" }),
      "解释一下",
    );
    await user.click(screen.getByRole("button", { name: "发送" }));
    await waitFor(() => expect(askFollowUpMock).toHaveBeenCalledOnce());

    expect(screen.getByRole("button", { name: "返回知识小窗" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "打开学习中心" })).toBeDisabled();
    await user.click(screen.getByRole("button", { name: "打开菜单" }));
    expect(screen.getByRole("button", { name: "模型设置" })).toBeDisabled();

    fireEvent.keyDown(window, { key: "Escape" });
    await waitFor(() =>
      expect(
        screen.queryByRole("navigation", { name: "主要页面" }),
      ).not.toBeInTheDocument(),
    );
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent(
      question ?? "",
    );

    expect(fireEvent.keyDown(window, { key: "Escape" })).toBe(false);
    expect(
      screen.getByText("AI 正在回答，请等待完成后再离开当前知识卡。"),
    ).toBeVisible();
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent(
      question ?? "",
    );

    await act(async () => {
      pending.resolve({
        answer: "这是保留下来的回答。",
        providerId: "deepseek",
        model: "deepseek-v4-flash",
        switchedFromProviderId: null,
      });
      await pending.promise;
    });

    expect(await screen.findByText("这是保留下来的回答。")).toBeVisible();
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: "返回知识小窗" }),
      ).toBeEnabled(),
    );
    const savedThread: FollowUpMessage[] = [
      { role: "user", content: "解释一下" },
      {
        role: "assistant",
        content: "这是保留下来的回答。",
        result: {
          answer: "这是保留下来的回答。",
          providerId: "deepseek",
          model: "deepseek-v4-flash",
          switchedFromProviderId: null,
        },
      },
    ];
    listCardFollowUpsMock.mockResolvedValue(savedThread);
    await user.click(screen.getByRole("button", { name: "返回知识小窗" }));
    expect(
      screen.getByRole("heading", { name: "想探索哪个领域？" }),
    ).toBeVisible();
    await user.click(screen.getByRole("button", { name: /浏览现有知识点/ }));
    await user.click(
      screen.getByRole("button", { name: "我想好了，揭晓答案" }),
    );
    expect(await screen.findByText("这是保留下来的回答。")).toBeVisible();
  });
});
