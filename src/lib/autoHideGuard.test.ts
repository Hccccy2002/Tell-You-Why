import { invoke } from "@tauri-apps/api/core";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  acquireAutoHideGuard,
  installAutoHideInteractionGuards,
  resetAutoHideGuards,
  withAutoHideGuard,
} from "./autoHideGuard";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue(undefined),
}));

const invokeMock = vi.mocked(invoke);

describe("auto-hide interaction guards", () => {
  let cleanupInteractionGuards: (() => void) | null = null;

  beforeEach(async () => {
    vi.useRealTimers();
    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      configurable: true,
      value: {},
    });
    invokeMock.mockClear();
    invokeMock.mockResolvedValue(undefined);
    await resetAutoHideGuards();
    invokeMock.mockClear();
  });

  afterEach(async () => {
    cleanupInteractionGuards?.();
    cleanupInteractionGuards = null;
    document.body.replaceChildren();
    vi.useRealTimers();
    await resetAutoHideGuards();
    Reflect.deleteProperty(window, "__TAURI_INTERNALS__");
  });

  it("keeps native auto-hide suspended until the last nested guard releases", async () => {
    const first = acquireAutoHideGuard("dialog");
    const second = acquireAutoHideGuard("menu");
    await Promise.all([first.ready, second.ready]);

    expect(invokeMock).toHaveBeenCalledTimes(1);
    expect(invokeMock).toHaveBeenLastCalledWith("set_auto_hide_suspended", {
      suspended: true,
    });

    await first.release();
    await first.release();
    expect(invokeMock).toHaveBeenCalledTimes(1);

    await second.release();
    expect(invokeMock).toHaveBeenLastCalledWith("set_auto_hide_suspended", {
      suspended: false,
    });
  });

  it("suspends before an async operation and always resumes", async () => {
    const order: string[] = [];
    invokeMock.mockImplementation((_command, args) => {
      order.push(
        (args as { suspended: boolean }).suspended ? "pause" : "resume",
      );
      return Promise.resolve(undefined);
    });

    await expect(
      withAutoHideGuard("file-picker", () => {
        order.push("operation");
        return Promise.reject(new Error("cancelled"));
      }),
    ).rejects.toThrow("cancelled");

    expect(order).toEqual(["pause", "operation", "resume"]);
  });

  it("bounds failed resume retries and forces recovery on window focus", async () => {
    cleanupInteractionGuards = installAutoHideInteractionGuards();
    const guard = acquireAutoHideGuard("resume-recovery");
    await guard.ready;
    invokeMock
      .mockRejectedValueOnce(new Error("resume failed once"))
      .mockRejectedValueOnce(new Error("resume failed twice"))
      .mockRejectedValueOnce(new Error("resume failed three times"));

    await guard.release();
    expect(
      invokeMock.mock.calls.filter(
        ([, args]) => !(args as { suspended: boolean }).suspended,
      ),
    ).toHaveLength(3);

    window.dispatchEvent(new FocusEvent("focus"));
    await vi.waitFor(() =>
      expect(
        invokeMock.mock.calls.filter(
          ([, args]) => !(args as { suspended: boolean }).suspended,
        ),
      ).toHaveLength(4),
    );
    expect(invokeMock).toHaveBeenLastCalledWith("set_auto_hide_suspended", {
      suspended: false,
    });
  });

  it("guards pointer holds and native popup controls", async () => {
    cleanupInteractionGuards = installAutoHideInteractionGuards();
    const select = document.createElement("select");
    document.body.append(select);

    select.dispatchEvent(
      new MouseEvent("pointerdown", {
        bubbles: true,
        button: 0,
      }),
    );
    await vi.waitFor(() =>
      expect(invokeMock).toHaveBeenLastCalledWith("set_auto_hide_suspended", {
        suspended: true,
      }),
    );

    window.dispatchEvent(new MouseEvent("pointerup"));
    select.dispatchEvent(new Event("change", { bubbles: true }));
    await vi.waitFor(() =>
      expect(invokeMock).toHaveBeenLastCalledWith("set_auto_hide_suspended", {
        suspended: false,
      }),
    );

    cleanupInteractionGuards();
    cleanupInteractionGuards = null;
  });

  it("keeps a select guarded when the previously focused control blurs", async () => {
    cleanupInteractionGuards = installAutoHideInteractionGuards();
    const input = document.createElement("input");
    const select = document.createElement("select");
    document.body.append(input, select);
    input.focus();

    select.dispatchEvent(
      new MouseEvent("pointerdown", {
        bubbles: true,
        button: 0,
        buttons: 1,
      }),
    );
    select.focus();
    window.dispatchEvent(
      new MouseEvent("pointerup", { bubbles: true, button: 0, buttons: 0 }),
    );

    await vi.waitFor(() =>
      expect(invokeMock).toHaveBeenLastCalledWith("set_auto_hide_suspended", {
        suspended: true,
      }),
    );
    expect(
      invokeMock.mock.calls.some(
        ([, args]) => !(args as { suspended: boolean }).suspended,
      ),
    ).toBe(false);

    select.dispatchEvent(new Event("change", { bubbles: true }));
    await vi.waitFor(() =>
      expect(invokeMock).toHaveBeenLastCalledWith("set_auto_hide_suspended", {
        suspended: false,
      }),
    );
  });

  it("releases a select guard when an unchanged or cancelled popup closes", async () => {
    vi.useFakeTimers();
    cleanupInteractionGuards = installAutoHideInteractionGuards();
    const select = document.createElement("select");
    const originalMatches = select.matches.bind(select);
    let popupOpen = false;
    vi.spyOn(select, "matches").mockImplementation((selector) =>
      selector === ":open" ? popupOpen : originalMatches(selector),
    );
    document.body.append(select);

    select.dispatchEvent(
      new MouseEvent("pointerdown", {
        bubbles: true,
        button: 0,
        buttons: 1,
      }),
    );
    popupOpen = true;
    window.dispatchEvent(
      new MouseEvent("pointerup", { bubbles: true, button: 0, buttons: 0 }),
    );
    await vi.advanceTimersByTimeAsync(50);
    expect(invokeMock).toHaveBeenLastCalledWith("set_auto_hide_suspended", {
      suspended: true,
    });

    popupOpen = false;
    await vi.advanceTimersByTimeAsync(50);
    expect(invokeMock).toHaveBeenLastCalledWith("set_auto_hide_suspended", {
      suspended: false,
    });
  });

  it("falls back safely when the select open selector is unsupported", async () => {
    vi.useFakeTimers();
    cleanupInteractionGuards = installAutoHideInteractionGuards();
    const select = document.createElement("select");
    const nextTarget = document.createElement("button");
    const originalMatches = select.matches.bind(select);
    vi.spyOn(select, "matches").mockImplementation((selector) => {
      if (selector === ":open") throw new DOMException("unsupported selector");
      return originalMatches(selector);
    });
    document.body.append(select, nextTarget);

    select.dispatchEvent(
      new MouseEvent("pointerdown", {
        bubbles: true,
        button: 0,
        buttons: 1,
      }),
    );
    window.dispatchEvent(
      new MouseEvent("pointerup", { bubbles: true, button: 0, buttons: 0 }),
    );
    await vi.advanceTimersByTimeAsync(10_000);
    expect(invokeMock).toHaveBeenLastCalledWith("set_auto_hide_suspended", {
      suspended: true,
    });

    nextTarget.dispatchEvent(
      new MouseEvent("pointerdown", {
        bubbles: true,
        button: 0,
        buttons: 1,
      }),
    );
    window.dispatchEvent(
      new MouseEvent("pointerup", { bubbles: true, button: 0, buttons: 0 }),
    );
    await vi.advanceTimersByTimeAsync(0);
    expect(invokeMock).toHaveBeenLastCalledWith("set_auto_hide_suspended", {
      suspended: false,
    });
  });

  it("keeps a time picker guarded while :open remains true and releases after unchanged close", async () => {
    vi.useFakeTimers();
    cleanupInteractionGuards = installAutoHideInteractionGuards();
    const timeInput = document.createElement("input");
    timeInput.type = "time";
    const originalMatches = timeInput.matches.bind(timeInput);
    let popupOpen = false;
    vi.spyOn(timeInput, "matches").mockImplementation((selector) =>
      selector === ":open" ? popupOpen : originalMatches(selector),
    );
    document.body.append(timeInput);
    timeInput.focus();

    timeInput.dispatchEvent(
      new MouseEvent("pointerdown", {
        bubbles: true,
        button: 0,
        buttons: 1,
      }),
    );
    popupOpen = true;
    window.dispatchEvent(
      new MouseEvent("pointerup", { bubbles: true, button: 0, buttons: 0 }),
    );
    await vi.advanceTimersByTimeAsync(2_100);
    expect(invokeMock).toHaveBeenLastCalledWith("set_auto_hide_suspended", {
      suspended: true,
    });
    expect(timeInput).toHaveFocus();

    popupOpen = false;
    await vi.advanceTimersByTimeAsync(50);
    expect(invokeMock).toHaveBeenLastCalledWith("set_auto_hide_suspended", {
      suspended: false,
    });
    expect(timeInput).toHaveFocus();
  });

  it("releases an input picker that remains closed through the opening grace", async () => {
    vi.useFakeTimers();
    cleanupInteractionGuards = installAutoHideInteractionGuards();
    const timeInput = document.createElement("input");
    timeInput.type = "time";
    const originalMatches = timeInput.matches.bind(timeInput);
    vi.spyOn(timeInput, "matches").mockImplementation((selector) =>
      selector === ":open" ? false : originalMatches(selector),
    );
    document.body.append(timeInput);

    timeInput.dispatchEvent(
      new MouseEvent("pointerdown", {
        bubbles: true,
        button: 0,
        buttons: 1,
      }),
    );
    window.dispatchEvent(
      new MouseEvent("pointerup", { bubbles: true, button: 0, buttons: 0 }),
    );
    await vi.advanceTimersByTimeAsync(450);
    expect(invokeMock).toHaveBeenLastCalledWith("set_auto_hide_suspended", {
      suspended: true,
    });

    await vi.advanceTimersByTimeAsync(50);
    expect(invokeMock).toHaveBeenLastCalledWith("set_auto_hide_suspended", {
      suspended: false,
    });
  });

  it("uses event fallback when an input picker rejects the open selector", async () => {
    vi.useFakeTimers();
    cleanupInteractionGuards = installAutoHideInteractionGuards();
    const timeInput = document.createElement("input");
    timeInput.type = "time";
    const nextTarget = document.createElement("button");
    const originalMatches = timeInput.matches.bind(timeInput);
    vi.spyOn(timeInput, "matches").mockImplementation((selector) => {
      if (selector === ":open") throw new DOMException("unsupported selector");
      return originalMatches(selector);
    });
    document.body.append(timeInput, nextTarget);

    timeInput.dispatchEvent(
      new MouseEvent("pointerdown", {
        bubbles: true,
        button: 0,
        buttons: 1,
      }),
    );
    window.dispatchEvent(
      new MouseEvent("pointerup", { bubbles: true, button: 0, buttons: 0 }),
    );
    await vi.advanceTimersByTimeAsync(5_000);
    expect(invokeMock).toHaveBeenLastCalledWith("set_auto_hide_suspended", {
      suspended: true,
    });

    nextTarget.dispatchEvent(
      new MouseEvent("pointerdown", {
        bubbles: true,
        button: 0,
        buttons: 1,
      }),
    );
    window.dispatchEvent(
      new MouseEvent("pointerup", { bubbles: true, button: 0, buttons: 0 }),
    );
    await vi.advanceTimersByTimeAsync(0);
    expect(invokeMock).toHaveBeenLastCalledWith("set_auto_hide_suspended", {
      suspended: false,
    });
  });

  it("releases a cancelled input picker when the app regains focus", async () => {
    cleanupInteractionGuards = installAutoHideInteractionGuards();
    const dateInput = document.createElement("input");
    dateInput.type = "date";
    document.body.append(dateInput);
    dateInput.dispatchEvent(
      new MouseEvent("pointerdown", {
        bubbles: true,
        button: 0,
        buttons: 1,
      }),
    );
    window.dispatchEvent(
      new MouseEvent("pointerup", { bubbles: true, button: 0, buttons: 0 }),
    );
    await vi.waitFor(() =>
      expect(invokeMock).toHaveBeenLastCalledWith("set_auto_hide_suspended", {
        suspended: true,
      }),
    );

    window.dispatchEvent(new FocusEvent("blur"));
    window.dispatchEvent(new FocusEvent("focus"));
    await vi.waitFor(() =>
      expect(invokeMock).toHaveBeenLastCalledWith("set_auto_hide_suspended", {
        suspended: false,
      }),
    );
  });

  it("does not release a pointer hold after an arbitrary timeout", async () => {
    vi.useFakeTimers();
    cleanupInteractionGuards = installAutoHideInteractionGuards();
    const target = document.createElement("div");
    document.body.append(target);

    target.dispatchEvent(
      new MouseEvent("pointerdown", {
        bubbles: true,
        button: 0,
        buttons: 1,
      }),
    );
    await vi.advanceTimersByTimeAsync(10_000);
    expect(invokeMock).toHaveBeenLastCalledWith("set_auto_hide_suspended", {
      suspended: true,
    });
    expect(
      invokeMock.mock.calls.some(
        ([, args]) => !(args as { suspended: boolean }).suspended,
      ),
    ).toBe(false);

    document.dispatchEvent(
      new MouseEvent("lostpointercapture", { bubbles: true, buttons: 1 }),
    );
    await Promise.resolve();
    expect(invokeMock).toHaveBeenLastCalledWith("set_auto_hide_suspended", {
      suspended: true,
    });

    window.dispatchEvent(
      new MouseEvent("pointerup", { bubbles: true, button: 0, buttons: 0 }),
    );
    await vi.advanceTimersByTimeAsync(0);
    expect(invokeMock).toHaveBeenLastCalledWith("set_auto_hide_suspended", {
      suspended: false,
    });
  });

  it("captures ordinary pointers until release but skips native popup controls", async () => {
    cleanupInteractionGuards = installAutoHideInteractionGuards();
    const target = document.createElement("div");
    const timeInput = document.createElement("input");
    timeInput.type = "time";
    const setCapture = vi.fn();
    const releaseCapture = vi.fn();
    const setNativeCapture = vi.fn();
    Object.defineProperties(target, {
      setPointerCapture: { configurable: true, value: setCapture },
      hasPointerCapture: { configurable: true, value: vi.fn(() => true) },
      releasePointerCapture: { configurable: true, value: releaseCapture },
    });
    Object.defineProperty(timeInput, "setPointerCapture", {
      configurable: true,
      value: setNativeCapture,
    });
    document.body.append(target, timeInput);

    const ordinaryDown = new MouseEvent("pointerdown", {
      bubbles: true,
      button: 0,
      buttons: 1,
    });
    Object.defineProperty(ordinaryDown, "pointerId", { value: 7 });
    target.dispatchEvent(ordinaryDown);
    expect(setCapture).toHaveBeenCalledWith(7);

    const ordinaryUp = new MouseEvent("pointerup", {
      bubbles: true,
      button: 0,
      buttons: 0,
    });
    Object.defineProperty(ordinaryUp, "pointerId", { value: 7 });
    window.dispatchEvent(ordinaryUp);
    await vi.waitFor(() => expect(releaseCapture).toHaveBeenCalledWith(7));

    const nativeDown = new MouseEvent("pointerdown", {
      bubbles: true,
      button: 0,
      buttons: 1,
    });
    Object.defineProperty(nativeDown, "pointerId", { value: 8 });
    timeInput.dispatchEvent(nativeDown);
    expect(setNativeCapture).not.toHaveBeenCalled();
    window.dispatchEvent(
      new MouseEvent("pointerup", { bubbles: true, buttons: 0 }),
    );
    timeInput.dispatchEvent(new Event("change", { bubbles: true }));
  });

  it("recovers a stale pointer guard after capture is lost with no buttons held", async () => {
    cleanupInteractionGuards = installAutoHideInteractionGuards();
    const target = document.createElement("div");
    document.body.append(target);
    target.dispatchEvent(
      new MouseEvent("pointerdown", {
        bubbles: true,
        button: 0,
        buttons: 1,
      }),
    );
    await vi.waitFor(() =>
      expect(invokeMock).toHaveBeenLastCalledWith("set_auto_hide_suspended", {
        suspended: true,
      }),
    );

    document.dispatchEvent(
      new MouseEvent("lostpointercapture", { bubbles: true, buttons: 0 }),
    );
    await vi.waitFor(() =>
      expect(invokeMock).toHaveBeenLastCalledWith("set_auto_hide_suspended", {
        suspended: false,
      }),
    );
  });

  it("recovers a pointer hold lost outside the webview without pointerup", async () => {
    cleanupInteractionGuards = installAutoHideInteractionGuards();
    const target = document.createElement("div");
    document.body.append(target);
    target.dispatchEvent(
      new MouseEvent("pointerdown", {
        bubbles: true,
        button: 0,
        buttons: 1,
      }),
    );
    await vi.waitFor(() =>
      expect(invokeMock).toHaveBeenLastCalledWith("set_auto_hide_suspended", {
        suspended: true,
      }),
    );

    window.dispatchEvent(new FocusEvent("blur"));
    await vi.waitFor(() =>
      expect(invokeMock).toHaveBeenLastCalledWith("set_auto_hide_suspended", {
        suspended: false,
      }),
    );
  });
});
