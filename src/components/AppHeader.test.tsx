import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { vi } from "vitest";
import { AppHeader } from "./AppHeader";

describe("AppHeader", () => {
  it("keeps the menu dismissible while locking navigation for an AI reply", async () => {
    const user = userEvent.setup();
    const onMenuToggle = vi.fn();
    const onNavigate = vi.fn();
    const { rerender } = render(
      <AppHeader
        view="home"
        menuOpen
        navigationLocked
        onMenuToggle={onMenuToggle}
        onNavigate={onNavigate}
      />,
    );

    expect(screen.getByRole("button", { name: "返回知识小窗" })).toBeDisabled();
    expect(screen.getByRole("status")).toHaveTextContent(
      "当前操作正在进行，完成后可切换页面。",
    );
    const navigationButtons = [
      "知识小窗",
      "兴趣设置",
      "模型设置",
      "收藏与历史",
      "通用设置",
      "质量评测",
    ].map((name) =>
      screen.getByRole("button", { name: new RegExp(`^${name}$`) }),
    );
    expect(navigationButtons).toHaveLength(6);
    for (const button of navigationButtons) {
      expect(button).toBeDisabled();
    }

    await user.click(screen.getByRole("button", { name: "关闭菜单" }));
    expect(onMenuToggle).toHaveBeenCalledOnce();
    await user.click(screen.getByRole("button", { name: "模型设置" }));
    expect(onNavigate).not.toHaveBeenCalled();

    rerender(
      <AppHeader
        view="home"
        menuOpen
        navigationLocked={false}
        onMenuToggle={onMenuToggle}
        onNavigate={onNavigate}
      />,
    );
    await user.click(screen.getByRole("button", { name: "模型设置" }));
    expect(onNavigate).toHaveBeenCalledWith("models");
    await user.click(screen.getByRole("button", { name: "质量评测" }));
    expect(onNavigate).toHaveBeenCalledWith("evaluation");
  });
});
