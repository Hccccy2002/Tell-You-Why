import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, vi } from "vitest";
import { bootstrapApp, clearData, recordInteraction } from "../lib/api";
import { LibraryScreen } from "./LibraryScreen";

describe("LibraryScreen", () => {
  beforeEach(async () => {
    await clearData("all");
  });

  it("retains a disliked card and deletes it only after confirmation", async () => {
    const bootstrap = await bootstrapApp();
    const card = bootstrap.card;
    if (!card) throw new Error("expected fallback card");
    await recordInteraction(card.id, "disliked");
    const onCardDeleted = vi.fn();
    const user = userEvent.setup();
    render(
      <LibraryScreen
        topics={bootstrap.topics}
        refreshToken={0}
        onOpenCard={vi.fn()}
        onCardDeleted={onCardDeleted}
        onHistoryCleared={vi.fn()}
      />,
    );

    await user.click(screen.getByRole("button", { name: "最近浏览" }));
    expect(await screen.findByText(card.question)).toBeVisible();
    await user.click(
      screen.getByRole("button", { name: "删除：" + card.question }),
    );
    const dialog = screen.getByRole("dialog", {
      name: "删除这条知识卡？",
    });
    expect(screen.getByText(card.question)).toBeVisible();
    expect(onCardDeleted).not.toHaveBeenCalled();

    await user.click(
      within(dialog).getByRole("button", { name: "删除知识卡" }),
    );
    await waitFor(() => {
      expect(screen.queryByText(card.question)).not.toBeInTheDocument();
    });
    expect(onCardDeleted).toHaveBeenCalledWith(card.id);
  });

  it("clears all recent history only after confirmation", async () => {
    const bootstrap = await bootstrapApp();
    const card = bootstrap.card;
    if (!card) throw new Error("expected fallback card");
    const onCardDeleted = vi.fn();
    const onHistoryCleared = vi.fn();
    const user = userEvent.setup();
    render(
      <LibraryScreen
        topics={bootstrap.topics}
        refreshToken={0}
        onOpenCard={vi.fn()}
        onCardDeleted={onCardDeleted}
        onHistoryCleared={onHistoryCleared}
      />,
    );

    await user.click(screen.getByRole("button", { name: "最近浏览" }));
    expect(await screen.findByText(card.question)).toBeVisible();
    await user.click(screen.getByRole("button", { name: "清除阅读记录" }));
    const dialog = screen.getByRole("dialog", {
      name: "清除所有阅读记录？",
    });
    expect(screen.getByText(card.question)).toBeVisible();
    expect(onHistoryCleared).not.toHaveBeenCalled();

    await user.click(
      within(dialog).getByRole("button", { name: "清除阅读记录" }),
    );
    expect(await screen.findByText("还没有浏览记录")).toBeVisible();
    expect(onCardDeleted).not.toHaveBeenCalled();
    expect(onHistoryCleared).toHaveBeenCalledOnce();
  });
});
