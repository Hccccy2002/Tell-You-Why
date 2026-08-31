import {
  askFollowUp,
  availableCardCount,
  bootstrapApp,
  clearData,
  deleteLibraryCard,
  friendlyError,
  listLibrary,
  nextCard,
  recordInteraction,
} from "./api";
import { fallbackCards } from "../data/fallbackCards";

describe("browser fallback core", () => {
  it("returns local content without network or a key", async () => {
    const bootstrap = await bootstrapApp();
    const first = bootstrap.card;
    expect(first).not.toBeNull();
    if (!first) throw new Error("expected fallback card");
    expect(first.question.length).toBeGreaterThan(0);
    expect(
      bootstrap.providers.every((provider) => !provider.keyConfigured),
    ).toBe(true);

    const next = await nextCard(first.id);
    expect(next.id).not.toBe(first.id);
  });

  it("does not fake follow-up answers in browser preview", async () => {
    await expect(
      askFollowUp(fallbackCards[0]!.id, "请继续说明", []),
    ).rejects.toThrow("请先配置模型哦~");
  });

  it("keeps favorites in the non-secret in-memory preview store", async () => {
    const bootstrap = await bootstrapApp();
    const card = bootstrap.card;
    if (!card) throw new Error("expected fallback card");
    await recordInteraction(card.id, "favorited");
    const refreshed = await bootstrapApp();
    expect(refreshed.card?.isFavorite).toBe(true);
  });

  it("lists a hidden history card after it is favorited without restoring the feed", async () => {
    await clearData("all");
    const bootstrap = await bootstrapApp();
    const card = bootstrap.card;
    if (!card) throw new Error("expected fallback card");
    const initialCount = await availableCardCount();

    await recordInteraction(card.id, "disliked");
    expect(await availableCardCount()).toBe(initialCount - 1);
    expect(
      (await listLibrary("history", null, "newest")).some(
        (item) => item.card.id === card.id,
      ),
    ).toBe(true);

    await recordInteraction(card.id, "favorited");
    const favorite = (await listLibrary("favorites", null, "newest")).find(
      (item) => item.card.id === card.id,
    );

    expect(favorite?.card.isFavorite).toBe(true);
    expect(favorite?.card.hiddenFromFeed).toBe(true);
    expect(await availableCardCount()).toBe(initialCount - 1);
  });

  it("keeps a disliked card in history until it is deleted", async () => {
    const bootstrap = await bootstrapApp();
    const card = bootstrap.card;
    if (!card) throw new Error("expected fallback card");
    const dislikedId = card.id;
    await recordInteraction(dislikedId, "disliked");

    let currentId = dislikedId;
    for (let index = 0; index < 24; index += 1) {
      const card = await nextCard(currentId);
      expect(card.id).not.toBe(dislikedId);
      currentId = card.id;
    }
    expect(
      (await listLibrary("history", null, "newest")).some(
        (item) => item.card.id === dislikedId,
      ),
    ).toBe(true);

    await deleteLibraryCard(dislikedId);
    expect(
      (await listLibrary("history", null, "newest")).some(
        (item) => item.card.id === dislikedId,
      ),
    ).toBe(false);
  });

  it("returns an empty feed after every local card is mastered", async () => {
    for (let index = 0; index < fallbackCards.length; index += 1) {
      const bootstrap = await bootstrapApp();
      if (!bootstrap.card) break;
      await recordInteraction(bootstrap.card.id, "known");
    }
    expect((await bootstrapApp()).card).toBeNull();
    await clearData("all");
  });

  it("normalizes unknown frontend errors", () => {
    expect(friendlyError({ unsafe: true })).toBe("操作没有完成，请稍后再试。");
  });
});
