import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { vi } from "vitest";
import { AppHeader } from "./AppHeader";

describe("AppHeader", () => {
  it("locks the study page entry while an operation is running", async () => {
    const onOpenStudy = vi.fn();
    render(
      <AppHeader
        view="home"
        menuOpen={false}
        navigationLocked
        onMenuToggle={vi.fn()}
        onNavigate={vi.fn()}
        onOpenStudy={onOpenStudy}
      />,
    );
    await userEvent.click(screen.getByRole("button", { name: "打开学习中心" }));
    expect(screen.getByRole("button", { name: "打开学习中心" })).toBeDisabled();
    expect(onOpenStudy).not.toHaveBeenCalled();
  });
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
    const developerTools = screen.getByText("开发者工具").closest("details");
    expect(developerTools).not.toHaveAttribute("open");
    const moreSettings = screen.getByText("更多设置").closest("details");
    expect(moreSettings).not.toHaveAttribute("open");
    await user.click(screen.getByText("更多设置"));
    expect(moreSettings).toHaveAttribute("open");
    await user.click(screen.getByText("开发者工具"));
    expect(developerTools).toHaveAttribute("open");
    const menu = screen.getByRole("navigation", { name: "主要页面" });
    const navigationButtons = [
      "知识小窗",
      "学习中心",
      "兴趣设置",
      "AI 模型",
      "MCP 服务器",
      "收藏与历史",
      "PDF 知识库",
      "设置",
      "质量评测",
    ].map((name) =>
      within(menu).getByRole("button", { name: new RegExp(`^${name}$`) }),
    );
    expect(navigationButtons).toHaveLength(9);
    for (const button of navigationButtons) {
      expect(button).toBeDisabled();
    }

    await user.click(screen.getByRole("button", { name: "关闭菜单" }));
    expect(onMenuToggle).toHaveBeenCalledOnce();
    await user.click(screen.getByRole("button", { name: "AI 模型" }));
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
    await user.click(screen.getByRole("button", { name: "AI 模型" }));
    expect(onNavigate).toHaveBeenCalledWith("models");
    await user.click(screen.getByRole("button", { name: "质量评测" }));
    expect(onNavigate).toHaveBeenCalledWith("evaluation");
  });
});
