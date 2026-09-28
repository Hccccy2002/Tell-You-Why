import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { vi } from "vitest";
import { fallbackCards, presetTopics } from "../data/fallbackCards";
import type { TopicPreference } from "../types";
import { Onboarding } from "./Onboarding";

const topics = presetTopics.map<TopicPreference>(([id, label], rank) => ({
  id,
  label,
  selected: false,
  enabled: true,
  custom: false,
  rank,
  weight: 0,
}));

describe("Onboarding", () => {
  it("requires three interests and completes without an API key", async () => {
    const user = userEvent.setup();
    const onComplete = vi.fn().mockResolvedValue(undefined);
    render(
      <Onboarding
        topics={topics}
        previewCard={fallbackCards[0]!}
        busy={false}
        onComplete={onComplete}
      />,
    );

    expect(screen.getByText(fallbackCards[0]!.question)).toBeVisible();
    await user.click(screen.getByRole("button", { name: "选择我的兴趣" }));

    const startButton = screen.getByRole("button", { name: "开始探索" });
    expect(startButton).toBeDisabled();
    await user.click(screen.getByRole("button", { name: "自然科学" }));
    await user.click(screen.getByRole("button", { name: "历史与文明" }));
    await user.click(screen.getByRole("button", { name: "计算机与互联网" }));
    expect(startButton).toBeEnabled();
    await user.click(startButton);

    expect(onComplete).toHaveBeenCalledWith({
      selectedTopicIds: [
        "natural_science",
        "history_civilization",
        "computing_internet",
      ],
      customInterests: [],
      reminderPreset: "manual",
    });
  });

  it("rejects high-risk custom topics", async () => {
    const user = userEvent.setup();
    render(
      <Onboarding
        topics={topics}
        previewCard={fallbackCards[0]!}
        busy={false}
        onComplete={vi.fn().mockResolvedValue(undefined)}
      />,
    );
    await user.click(screen.getByRole("button", { name: "选择我的兴趣" }));
    await user.type(screen.getByLabelText("自定义兴趣（可选）"), "投资建议");
    await user.click(screen.getByRole("button", { name: "添加" }));
    expect(screen.getByRole("alert")).toHaveTextContent(
      "不在 MVP 的内容范围内",
    );
  });
});
