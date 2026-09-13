import { StrictMode, useState } from "react";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { vi } from "vitest";
import { ConfirmationDialog } from "./ConfirmationDialog";

function DialogHarness() {
  const [open, setOpen] = useState(false);
  return (
    <>
      <button type="button" onClick={() => setOpen(true)}>
        打开危险操作
      </button>
      {open ? (
        <ConfirmationDialog
          id="test-confirmation"
          title="删除测试数据？"
          confirmLabel="删除测试数据"
          busyLabel="正在删除…"
          busy={false}
          onCancel={() => setOpen(false)}
          onConfirm={() => undefined}
        >
          <p>删除后无法恢复。</p>
        </ConfirmationDialog>
      ) : null}
    </>
  );
}

describe("ConfirmationDialog", () => {
  it("shows a notice with one focused acknowledgement button", async () => {
    const onCancel = vi.fn();
    const onConfirm = vi.fn();
    render(
      <ConfirmationDialog
        id="notice"
        variant="notice"
        eyebrow="随机学习"
        title="学习提示"
        confirmLabel="知道了"
        busyLabel="知道了"
        busy={false}
        onCancel={onCancel}
        onConfirm={onConfirm}
      >
        <p>暂时没有新的知识卡了哦~请新增一张~</p>
      </ConfirmationDialog>,
    );
    const user = userEvent.setup();
    const acknowledge = screen.getByRole("button", { name: "知道了" });
    expect(screen.getAllByRole("button")).toHaveLength(1);
    expect(acknowledge).toHaveFocus();
    await user.tab();
    expect(acknowledge).toHaveFocus();
    await user.click(acknowledge);
    expect(onConfirm).toHaveBeenCalledTimes(1);
    await user.keyboard("{Escape}");
    expect(onCancel).toHaveBeenCalledTimes(1);
  });
  it("traps focus, closes on Escape, and restores the trigger focus", async () => {
    const user = userEvent.setup();
    const appRoot = document.createElement("div");
    appRoot.id = "root";
    document.body.append(appRoot);
    render(
      <StrictMode>
        <DialogHarness />
      </StrictMode>,
      { container: appRoot },
    );

    const trigger = screen.getByRole("button", { name: "打开危险操作" });
    await user.click(trigger);

    const cancel = screen.getByRole("button", { name: "取消" });
    const confirm = screen.getByRole("button", { name: "删除测试数据" });
    await waitFor(() => expect(cancel).toHaveFocus());
    expect(document.getElementById("root")).toHaveProperty("inert", true);

    await user.tab();
    expect(confirm).toHaveFocus();
    await user.tab();
    expect(cancel).toHaveFocus();
    await user.tab({ shift: true });
    expect(confirm).toHaveFocus();

    fireEvent.keyDown(document, { key: "Escape" });
    await waitFor(() =>
      expect(
        screen.queryByRole("dialog", { name: "删除测试数据？" }),
      ).not.toBeInTheDocument(),
    );
    await waitFor(() => expect(trigger).toHaveFocus());
    expect(document.getElementById("root")).toHaveProperty("inert", false);
    appRoot.remove();
  });

  it("cannot be dismissed or submitted again while busy", () => {
    const onCancel = vi.fn();
    const onConfirm = vi.fn();
    render(
      <ConfirmationDialog
        id="busy-confirmation"
        title="清除全部数据？"
        confirmLabel="清除全部数据"
        busyLabel="正在清除…"
        busy
        onCancel={onCancel}
        onConfirm={onConfirm}
      >
        <p>请等待操作完成。</p>
      </ConfirmationDialog>,
    );

    const cancel = screen.getByRole("button", { name: "取消" });
    const confirm = screen.getByRole("button", { name: "正在清除…" });
    expect(cancel).toBeDisabled();
    expect(confirm).toBeDisabled();

    fireEvent.keyDown(document, { key: "Escape" });
    const backdrop = document.querySelector(".confirmation-backdrop");
    expect(backdrop).not.toBeNull();
    fireEvent.click(backdrop!);
    fireEvent.click(cancel);
    fireEvent.click(confirm);

    expect(onCancel).not.toHaveBeenCalled();
    expect(onConfirm).not.toHaveBeenCalled();
  });
});
