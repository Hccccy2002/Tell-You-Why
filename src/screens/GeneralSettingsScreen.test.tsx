import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { vi } from "vitest";
import { bootstrapApp } from "../lib/api";
import { GeneralSettingsScreen } from "./GeneralSettingsScreen";

describe("GeneralSettingsScreen", () => {
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
});
