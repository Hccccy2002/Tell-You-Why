import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, vi } from "vitest";
import { SearchSettingsPanel } from "./SearchSettingsPanel";
import type * as SearchModule from "../lib/search";
import {
  getSearchSettings,
  saveSearchKey,
  deleteSearchKey,
  testSearchConnection,
  saveSearchOptions,
} from "../lib/search";

vi.mock("../lib/search", async (importOriginal) => ({
  ...(await importOriginal<typeof SearchModule>()),
  getSearchSettings: vi.fn(),
  saveSearchKey: vi.fn(),
  deleteSearchKey: vi.fn(),
  testSearchConnection: vi.fn(),
  saveSearchOptions: vi.fn(),
}));
const configured = {
  keyConfigured: true,
  keyLast4: "1234",
  connectionVerified: false,
};

beforeEach(() => {
  vi.resetAllMocks();
  vi.mocked(getSearchSettings).mockResolvedValue({
    keyConfigured: false,
    keyLast4: null,
    connectionVerified: false,
  });
  vi.mocked(saveSearchKey).mockResolvedValue(configured);
  vi.mocked(deleteSearchKey).mockResolvedValue(undefined);
  vi.mocked(testSearchConnection).mockResolvedValue({
    ...configured,
    connectionVerified: true,
  });
  vi.mocked(saveSearchOptions).mockResolvedValue(configured);
});

it("starts with search off and saves explicit mode and bounded daily limits", async () => {
  const user = userEvent.setup();
  render(<SearchSettingsPanel onBusyChange={vi.fn()} />);
  await waitFor(() => expect(screen.getByLabelText("搜索模式")).toBeEnabled());
  expect(screen.getByLabelText("搜索模式")).toHaveValue("off");
  await user.selectOptions(screen.getByLabelText("搜索模式"), "auto");
  await user.clear(screen.getByLabelText("每日搜索次数上限"));
  await user.type(screen.getByLabelText("每日搜索次数上限"), "20");
  expect(saveSearchOptions).not.toHaveBeenCalled();
  await user.click(screen.getByRole("button", { name: "保存搜索设置" }));
  expect(saveSearchOptions).toHaveBeenCalledWith({
    mode: "auto",
    engine: "search_pro",
    dailyAttemptLimit: 20,
  });
});

it("saves a password only on request, clears the input and tests the stored key", async () => {
  const user = userEvent.setup();
  render(<SearchSettingsPanel onBusyChange={vi.fn()} />);
  const input = screen.getByLabelText("智谱 API Key");
  await waitFor(() => expect(input).toBeEnabled());
  expect(input).toHaveAttribute("type", "password");
  await user.type(input, "test-zhipu-key-1234");
  expect(saveSearchKey).not.toHaveBeenCalled();
  await user.click(screen.getByRole("button", { name: "保存智谱配置" }));
  await waitFor(() => expect(input).toHaveValue(""));
  expect(saveSearchKey).toHaveBeenCalledWith("test-zhipu-key-1234", false);
  expect(input).toHaveAttribute("placeholder", "已配置 · •••• 1234");
  await user.click(screen.getByRole("button", { name: "测试智谱连接" }));
  expect(await screen.findByText("连接已验证")).toBeVisible();
  expect(testSearchConnection).toHaveBeenCalledWith();
});

it("requires confirmation for replacement and deletion", async () => {
  vi.mocked(getSearchSettings).mockResolvedValue(configured);
  const user = userEvent.setup();
  render(<SearchSettingsPanel onBusyChange={vi.fn()} />);
  await screen.findByText("尚未验证连接");
  await user.type(
    screen.getByLabelText("智谱 API Key"),
    "replacement-key-5678",
  );
  expect(screen.getByRole("button", { name: "测试智谱连接" })).toBeDisabled();
  await user.click(screen.getByRole("button", { name: "保存智谱配置" }));
  const replace = screen.getByRole("dialog");
  expect(saveSearchKey).not.toHaveBeenCalled();
  await user.click(
    within(replace).getByRole("button", { name: "确认覆盖 API Key" }),
  );
  await waitFor(() =>
    expect(saveSearchKey).toHaveBeenCalledWith("replacement-key-5678", true),
  );
  await user.click(screen.getByRole("button", { name: "删除智谱 API Key" }));
  expect(deleteSearchKey).not.toHaveBeenCalled();
  await user.click(
    within(screen.getByRole("dialog")).getByRole("button", {
      name: "删除 API Key",
    }),
  );
  expect(await screen.findByText("智谱系统凭据已删除。")).toBeVisible();
});

it("keeps an unsuccessful save editable and does not claim success", async () => {
  vi.mocked(saveSearchKey).mockRejectedValue(new Error("系统凭据库暂时不可用"));
  const user = userEvent.setup();
  render(<SearchSettingsPanel onBusyChange={vi.fn()} />);
  const input = screen.getByLabelText("智谱 API Key");
  await waitFor(() => expect(input).toBeEnabled());
  await user.type(input, "test-key-1234");
  await user.click(screen.getByRole("button", { name: "保存智谱配置" }));
  expect(await screen.findByText("系统凭据库暂时不可用")).toBeVisible();
  expect(input).toHaveValue("test-key-1234");
  expect(
    screen.queryByRole("button", { name: "测试智谱连接" }),
  ).not.toBeInTheDocument();
});
