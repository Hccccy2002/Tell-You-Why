import {
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, vi } from "vitest";
import { bootstrapApp, clearData } from "../lib/api";
import type { DataClearScope } from "../types";
import { GeneralSettingsScreen } from "./GeneralSettingsScreen";

vi.mock("../lib/api", async (importOriginal) => {
  const actual = await importOriginal();
  if (!actual || typeof actual !== "object") {
    throw new Error("Failed to load the real API module for this test");
  }
  return {
    ...actual,
    clearData: vi.fn().mockResolvedValue(null),
  };
});

const clearDataMock = vi.mocked(clearData);

const clearCases: Array<{
  scope: DataClearScope;
  entryName: RegExp;
  title: string;
  confirmLabel: string;
}> = [
  {
    scope: "history",
    entryName: /清除阅读记录/,
    title: "清除所有阅读记录？",
    confirmLabel: "清除阅读记录",
  },
  {
    scope: "preferences",
    entryName: /清除偏好/,
    title: "清除推荐偏好？",
    confirmLabel: "清除偏好",
  },
  {
    scope: "all",
    entryName: /清除全部本地数据/,
    title: "清除全部本地数据？",
    confirmLabel: "清除全部本地数据",
  },
];

describe("GeneralSettingsScreen", () => {
  beforeEach(() => {
    clearDataMock.mockClear();
  });

  it("accepts a custom generation total and shows generated/total usage", async () => {
    const bootstrap = await bootstrapApp();
    const onSaved = vi.fn();
    const user = userEvent.setup();
    render(
      <GeneralSettingsScreen
        initialSettings={bootstrap.settings}
        onSaved={onSaved}
        onDataCleared={vi.fn()}
      />,
    );

    await user.click(screen.getByText("生成额度"));
    expect(await screen.findByText("已生成 0/10")).toBeVisible();
    const input = screen.getByRole("spinbutton", {
      name: "每日生成总数",
    });
    input.focus();
    fireEvent.wheel(input, { deltaY: -100 });
    expect(input).not.toHaveFocus();
    expect(input).toHaveValue(10);
    await user.clear(input);
    await user.type(input, "37");
    expect(screen.getByText("已生成 0/37")).toBeVisible();
    await user.click(screen.getByRole("button", { name: "保存通用设置" }));
    expect(onSaved).toHaveBeenCalledWith(
      expect.objectContaining({ dailyGenerationLimit: 37 }),
    );
  });

  it("saves mouse-leave auto-hide only after the settings form is submitted", async () => {
    const bootstrap = await bootstrapApp();
    const onSaved = vi.fn();
    const user = userEvent.setup();
    render(
      <GeneralSettingsScreen
        initialSettings={{
          ...bootstrap.settings,
          autoHideOnMouseLeave: false,
        }}
        onSaved={onSaved}
        onDataCleared={vi.fn()}
      />,
    );

    await user.click(screen.getByText("窗口与启动"));
    const autoHide = screen.getByRole("checkbox", {
      name: /鼠标移出后自动收起/,
    });
    expect(autoHide).not.toBeChecked();

    await user.click(autoHide);
    expect(autoHide).toBeChecked();
    expect(onSaved).not.toHaveBeenCalled();

    await user.click(screen.getByRole("button", { name: "保存通用设置" }));
    expect(onSaved).toHaveBeenCalledWith(
      expect.objectContaining({ autoHideOnMouseLeave: true }),
    );
  });

  it.each(clearCases)(
    "clears $scope only after cancelling and confirming the reopened dialog",
    async ({ scope, entryName, title, confirmLabel }) => {
      const bootstrap = await bootstrapApp();
      const onDataCleared = vi.fn();
      const user = userEvent.setup();
      render(
        <GeneralSettingsScreen
          initialSettings={bootstrap.settings}
          onSaved={vi.fn()}
          onDataCleared={onDataCleared}
        />,
      );

      await user.click(screen.getByText("内容与数据"));
      const entry = screen.getByRole("button", { name: entryName });
      await user.click(entry);
      const firstDialog = screen.getByRole("dialog", { name: title });

      expect(clearDataMock).not.toHaveBeenCalled();
      expect(onDataCleared).not.toHaveBeenCalled();
      await user.click(
        within(firstDialog).getByRole("button", { name: "取消" }),
      );
      expect(
        screen.queryByRole("dialog", { name: title }),
      ).not.toBeInTheDocument();
      expect(clearDataMock).not.toHaveBeenCalled();
      expect(onDataCleared).not.toHaveBeenCalled();

      await user.click(entry);
      const reopenedDialog = screen.getByRole("dialog", { name: title });
      await user.click(
        within(reopenedDialog).getByRole("button", { name: confirmLabel }),
      );

      await waitFor(() => {
        expect(clearDataMock).toHaveBeenCalledTimes(1);
        expect(onDataCleared).toHaveBeenCalledTimes(1);
      });
      expect(clearDataMock).toHaveBeenCalledWith(scope);
      expect(onDataCleared).toHaveBeenCalledWith(scope, null);
    },
  );

  it("reloads cleared data and surfaces a post-clear cleanup warning", async () => {
    const warning =
      "本地数据已清除，但 1 项系统凭据暂未删除，将在下次操作时重试。";
    clearDataMock.mockResolvedValueOnce(warning);
    const bootstrap = await bootstrapApp();
    const onDataCleared = vi.fn();
    const user = userEvent.setup();
    render(
      <GeneralSettingsScreen
        initialSettings={bootstrap.settings}
        onSaved={vi.fn()}
        onDataCleared={onDataCleared}
      />,
    );

    await user.click(screen.getByText("内容与数据"));
    await user.click(screen.getByRole("button", { name: /清除全部本地数据/ }));
    const dialog = screen.getByRole("dialog", {
      name: "清除全部本地数据？",
    });
    await user.click(
      within(dialog).getByRole("button", { name: "清除全部本地数据" }),
    );

    await waitFor(() => {
      expect(onDataCleared).toHaveBeenCalledWith("all", warning);
    });
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent(warning);
  });

  it("keeps a failed clear confirmation open and retries only on another click", async () => {
    clearDataMock
      .mockRejectedValueOnce(new Error("数据库暂时忙碌"))
      .mockResolvedValueOnce(null);
    const bootstrap = await bootstrapApp();
    const onDataCleared = vi.fn();
    const user = userEvent.setup();
    render(
      <GeneralSettingsScreen
        initialSettings={bootstrap.settings}
        onSaved={vi.fn()}
        onDataCleared={onDataCleared}
      />,
    );

    await user.click(screen.getByText("内容与数据"));
    await user.click(screen.getByRole("button", { name: /清除阅读记录/ }));
    const dialog = screen.getByRole("dialog", {
      name: "清除所有阅读记录？",
    });
    const confirm = within(dialog).getByRole("button", {
      name: "清除阅读记录",
    });
    await user.click(confirm);

    expect(await within(dialog).findByRole("alert")).toHaveTextContent(
      "数据库暂时忙碌",
    );
    expect(clearDataMock).toHaveBeenCalledTimes(1);
    expect(onDataCleared).not.toHaveBeenCalled();

    await user.click(confirm);
    await waitFor(() => {
      expect(clearDataMock).toHaveBeenCalledTimes(2);
      expect(onDataCleared).toHaveBeenCalledWith("history", null);
    });
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });
});
