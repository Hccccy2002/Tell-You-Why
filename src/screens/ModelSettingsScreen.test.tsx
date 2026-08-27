import { render, screen } from "@testing-library/react";
import { vi } from "vitest";
import { bootstrapApp } from "../lib/api";
import { ModelSettingsScreen } from "./ModelSettingsScreen";

describe("ModelSettingsScreen", () => {
  it("keeps generation user-triggered and does not expose auto replenishment", async () => {
    const bootstrap = await bootstrapApp();
    render(
      <ModelSettingsScreen
        initialProviders={bootstrap.providers}
        onProvidersChanged={vi.fn()}
      />,
    );

    expect(screen.queryByText("自动补充内容")).not.toBeInTheDocument();
    expect(screen.queryByText("立即补充 5 张")).not.toBeInTheDocument();
    expect(screen.getByText(/不会后台自动补充/)).toBeVisible();
  });
});
