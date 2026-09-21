import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { vi } from "vitest";
import { fallbackCards } from "../data/fallbackCards";
import {
  cancelSearchFollowUp,
  getSearchSettings,
  prepareSearchFollowUp,
  searchFollowUpStatus,
} from "../lib/search";
import type { FollowUpResult } from "../types";
import { KnowledgeCardView } from "./KnowledgeCardView";

vi.mock("../lib/search", () => ({
  getSearchSettings: vi.fn(),
  prepareSearchFollowUp: vi.fn(),
  searchFollowUpStatus: vi.fn(),
  cancelSearchFollowUp: vi.fn(),
}));
beforeEach(() => {
  vi.mocked(getSearchSettings).mockResolvedValue({
    keyConfigured: true,
    keyLast4: "mock",
    connectionVerified: true,
    options: { mode: "off", engine: "search_pro", dailyAttemptLimit: 50 },
  });
  vi.mocked(prepareSearchFollowUp).mockResolvedValue("run-123");
  vi.mocked(searchFollowUpStatus).mockResolvedValue("searching");
  vi.mocked(cancelSearchFollowUp).mockResolvedValue(true);
});

it("prepares an explicit search once and keeps the navigation lock until cancellation settles", async () => {
  let reject!: (e: Error) => void;
  const pending = new Promise<FollowUpResult>((_, fail) => {
    reject = fail;
  });
  const ask = vi.fn().mockReturnValue(pending);
  const busy = vi.fn();
  const user = userEvent.setup();
  const noop = vi.fn();
  render(
    <KnowledgeCardView
      card={fallbackCards[0]!}
      availableCardCount={8}
      busy={false}
      canGoPrevious={false}
      initialRevealed
      onAskFollowUp={ask}
      onLoadFollowUps={() => Promise.resolve([])}
      onFollowUpBusyChange={busy}
      onInteraction={async () => {}}
      onReturnHome={noop}
      onPrevious={noop}
      onNext={async () => {}}
      onDismiss={async () => {}}
      onMaster={async () => {}}
      onGenerateSameTopic={async () => {}}
      onGenerateRandomTopic={async () => {}}
    />,
  );
  await waitFor(() =>
    expect(screen.getByRole("textbox", { name: "输入追问" })).toBeEnabled(),
  );
  await user.click(screen.getByRole("checkbox", { name: "本次联网核查" }));
  await user.type(
    screen.getByRole("textbox", { name: "输入追问" }),
    "核查 TCP 协议",
  );
  await user.click(screen.getByRole("button", { name: "发送" }));
  expect(prepareSearchFollowUp).toHaveBeenCalledWith(
    fallbackCards[0]!.id,
    true,
  );
  expect(ask).toHaveBeenCalledWith(
    "核查 TCP 协议",
    [],
    "核查 TCP 协议",
    "run-123",
  );
  await user.click(await screen.findByRole("button", { name: "取消本次查询" }));
  expect(cancelSearchFollowUp).toHaveBeenCalledWith(
    fallbackCards[0]!.id,
    "run-123",
  );
  expect(screen.getByRole("button", { name: "返回主界面" })).toBeDisabled();
  await act(async () => {
    reject(new Error("本次搜索已取消"));
    await pending.catch(() => undefined);
  });
  expect(await screen.findByRole("alert")).toHaveTextContent("本次搜索已取消");
  expect(screen.getByRole("textbox", { name: "输入追问" })).toHaveValue(
    "核查 TCP 协议",
  );
  expect(screen.getByRole("button", { name: "返回主界面" })).toBeEnabled();
  expect(busy).toHaveBeenLastCalledWith(false);
  expect(screen.queryByText("AI 回答")).not.toBeInTheDocument();
});
