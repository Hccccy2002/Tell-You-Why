import { fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { vi } from "vitest";
import { presetTopics } from "../data/fallbackCards";
import type { TopicPreference } from "../types";
import { InterestSettingsScreen } from "./InterestSettingsScreen";

const topics = presetTopics.map<TopicPreference>(([id, label], rank) => ({
  id,
  label,
  selected: rank < 3,
  enabled: true,
  custom: false,
  rank,
  weight: 0,
}));

describe("InterestSettingsScreen", () => {
  it("moves an interest to first with the pin button and updates its weight", async () => {
    const user = userEvent.setup();
    const onSaved = vi.fn();
    render(
      <InterestSettingsScreen
        initialTopics={topics}
        personalizationEnabled={true}
        onSaved={onSaved}
      />,
    );

    await user.click(screen.getByRole("button", { name: "置顶 历史与文明" }));

    const list = screen.getByRole("list", { name: "兴趣权重排序" });
    const rows = within(list).getAllByRole("listitem");
    expect(rows[0]).toHaveTextContent("历史与文明");
    expect(rows[0]).toHaveTextContent(`生成权重 ${topics.length}`);
    expect(
      within(rows[0]!).getByRole("button", { name: "置顶 历史与文明" }),
    ).toBeDisabled();

    await user.click(screen.getByRole("button", { name: "保存兴趣设置" }));
    const saved = onSaved.mock.calls[0]?.[0] as TopicPreference[];
    expect(saved[0]?.id).toBe("history_civilization");
    expect(saved[0]?.rank).toBe(0);
  });

  it("reorders in real time with the mouse and saves position-based weights", async () => {
    const user = userEvent.setup();
    const onSaved = vi.fn();
    render(
      <InterestSettingsScreen
        initialTopics={topics}
        personalizationEnabled={true}
        onSaved={onSaved}
      />,
    );

    const list = screen.getByRole("list", { name: "兴趣权重排序" });
    const rows = within(list).getAllByRole("listitem");
    const firstRow = rows[0]!;
    const thirdRow = rows[2]!;
    expect(firstRow).not.toHaveAttribute("draggable");
    vi.spyOn(thirdRow, "getBoundingClientRect").mockReturnValue({
      top: 0,
      bottom: 48,
      height: 48,
      left: 0,
      right: 300,
      width: 300,
      x: 0,
      y: 0,
      toJSON: () => ({}),
    });
    Object.defineProperty(document, "elementFromPoint", {
      configurable: true,
      value: vi.fn(() => thirdRow),
    });
    const pointerEvent = (type: string, clientY: number, button = 0) => {
      const event = new Event(type, { bubbles: true, cancelable: true });
      Object.defineProperties(event, {
        pointerId: { value: 1 },
        button: { value: button },
        clientX: { value: 20 },
        clientY: { value: clientY },
      });
      return event;
    };
    fireEvent(firstRow, pointerEvent("pointerdown", 2));
    fireEvent(firstRow, pointerEvent("pointermove", 40));
    fireEvent(firstRow, pointerEvent("pointerup", 40));

    const labels = within(list)
      .getAllByRole("listitem")
      .slice(0, 3)
      .map(
        (row) => within(row).getByRole("checkbox").parentElement?.textContent,
      );
    expect(labels[0]).toContain("宇宙与地球");
    expect(labels[1]).toContain("历史与文明");
    expect(labels[2]).toContain("自然科学");
    expect(labels[0]).toContain(`生成权重 ${topics.length}`);
    expect(labels[1]).toContain(`生成权重 ${topics.length - 1}`);
    expect(labels[2]).toContain(`生成权重 ${topics.length - 2}`);

    await user.click(screen.getByRole("button", { name: "保存兴趣设置" }));
    const saved = onSaved.mock.calls[0]?.[0] as TopicPreference[];
    expect(saved.slice(0, 3).map((topic) => topic.id)).toEqual([
      "space_earth",
      "history_civilization",
      "natural_science",
    ]);
    expect(saved.slice(0, 3).map((topic) => topic.rank)).toEqual([0, 1, 2]);
  });
});
