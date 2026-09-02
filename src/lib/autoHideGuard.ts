import { invoke } from "@tauri-apps/api/core";
import { useEffect } from "react";

interface AutoHideGuard {
  ready: Promise<void>;
  release: () => Promise<void>;
}

const activeGuards = new Map<symbol, string>();
let nativeSuspended = false;
let syncQueue = Promise.resolve();
const NATIVE_SYNC_MAX_ATTEMPTS = 3;

function isDesktopRuntime() {
  return typeof window !== "undefined" && window.__TAURI_INTERNALS__ != null;
}

function queueNativeSync(force = false): Promise<void> {
  syncQueue = syncQueue
    .catch(() => undefined)
    .then(async () => {
      const shouldSuspend = activeGuards.size > 0;
      if (!force && shouldSuspend === nativeSuspended) return;
      if (!isDesktopRuntime()) {
        nativeSuspended = shouldSuspend;
        return;
      }
      for (let attempt = 0; attempt < NATIVE_SYNC_MAX_ATTEMPTS; attempt += 1) {
        try {
          await invoke("set_auto_hide_suspended", {
            suspended: shouldSuspend,
          });
          nativeSuspended = shouldSuspend;
          return;
        } catch {
          // Retry a bounded number of times. A later focus event also forces a sync.
        }
      }
      nativeSuspended = !shouldSuspend;
    });
  return syncQueue;
}

export function acquireAutoHideGuard(reason: string): AutoHideGuard {
  const token = Symbol(reason);
  activeGuards.set(token, reason);
  const ready = queueNativeSync();
  let released = false;
  return {
    ready,
    release() {
      if (released) return syncQueue;
      released = true;
      activeGuards.delete(token);
      return queueNativeSync();
    },
  };
}

export async function withAutoHideGuard<T>(
  reason: string,
  operation: () => Promise<T>,
): Promise<T> {
  const guard = acquireAutoHideGuard(reason);
  await guard.ready;
  try {
    return await operation();
  } finally {
    await guard.release();
  }
}

export function useAutoHideGuard(active: boolean, reason: string) {
  useEffect(() => {
    if (!active) return;
    const guard = acquireAutoHideGuard(reason);
    return () => {
      void guard.release();
    };
  }, [active, reason]);
}

export async function resetAutoHideGuards() {
  activeGuards.clear();
  await queueNativeSync(true);
}

function nativePopupControlFor(
  target: EventTarget | null,
): HTMLSelectElement | HTMLInputElement | null {
  if (!(target instanceof Element)) return null;
  const control = target.closest("select, input");
  if (control instanceof HTMLSelectElement) return control;
  if (!(control instanceof HTMLInputElement)) return null;
  return ["date", "datetime-local", "month", "time", "week", "color"].includes(
    control.type,
  )
    ? control
    : null;
}

const NATIVE_POPUP_POLL_INTERVAL = 50;
const NATIVE_POPUP_OPEN_GRACE = 500;

type NativePopupControl = HTMLSelectElement | HTMLInputElement;

interface NativeControlGuard {
  control: NativePopupControl;
  kind: "select" | "input-picker";
  guard: AutoHideGuard;
  openedAt: number;
  openObserved: boolean;
  openSupport: "unknown" | "supported" | "unsupported";
  openingClickSeen: boolean;
  pollTimer: number | null;
  controlBlurred: boolean;
  windowBlurred: boolean;
}

function nativePopupIsOpen(control: NativePopupControl): boolean | null {
  try {
    return control.matches(":open");
  } catch {
    return null;
  }
}

export function installAutoHideInteractionGuards() {
  let pointerGuard: AutoHideGuard | null = null;
  const capturedPointers = new Map<number, Element>();
  let nativeControlGuard: NativeControlGuard | null = null;

  function releaseCapturedPointers() {
    for (const [pointerId, target] of capturedPointers) {
      capturedPointers.delete(pointerId);
      try {
        if (
          typeof target.releasePointerCapture === "function" &&
          (typeof target.hasPointerCapture !== "function" ||
            target.hasPointerCapture(pointerId))
        ) {
          target.releasePointerCapture(pointerId);
        }
      } catch {
        // Capture may already have been released by the browser.
      }
    }
  }

  function releasePointerGuard() {
    releaseCapturedPointers();
    if (pointerGuard) {
      void pointerGuard.release();
      pointerGuard = null;
    }
  }

  function holdPointerGuard(event: PointerEvent) {
    if (event.button < 0) return;
    if (
      !nativePopupControlFor(event.target) &&
      event.target instanceof Element &&
      Number.isInteger(event.pointerId) &&
      event.pointerId >= 0 &&
      typeof event.target.setPointerCapture === "function"
    ) {
      try {
        event.target.setPointerCapture(event.pointerId);
        capturedPointers.set(event.pointerId, event.target);
      } catch {
        // Some WebView controls reject capture even during pointerdown.
      }
    }
    pointerGuard ??= acquireAutoHideGuard("pointer-interaction");
  }

  function releasePointerGuardIfIdle(event: Event) {
    const buttons = "buttons" in event ? event.buttons : 0;
    if (typeof buttons === "number" && buttons !== 0) return;
    releasePointerGuard();
  }

  function releaseNativeControlGuard(expectedControl?: NativePopupControl) {
    const current = nativeControlGuard;
    if (!current || (expectedControl && current.control !== expectedControl)) {
      return;
    }
    nativeControlGuard = null;
    if (current.pollTimer != null) window.clearTimeout(current.pollTimer);
    void current.guard.release();
  }

  function pollNativeControl(current: NativeControlGuard) {
    if (nativeControlGuard !== current) return;
    const open = nativePopupIsOpen(current.control);
    if (open == null) {
      current.openSupport = "unsupported";
      current.pollTimer = null;
      return;
    }
    current.openSupport = "supported";
    if (open) {
      current.openObserved = true;
    } else if (
      current.openObserved ||
      Date.now() - current.openedAt >= NATIVE_POPUP_OPEN_GRACE
    ) {
      releaseNativeControlGuard(current.control);
      return;
    }
    current.pollTimer = window.setTimeout(
      () => pollNativeControl(current),
      NATIVE_POPUP_POLL_INTERVAL,
    );
  }

  function acquireNativeControlGuard(control: NativePopupControl) {
    releaseNativeControlGuard();
    const current: NativeControlGuard = {
      control,
      kind: control instanceof HTMLSelectElement ? "select" : "input-picker",
      guard: acquireAutoHideGuard("native-popup-control"),
      openedAt: Date.now(),
      openObserved: false,
      openSupport: "unknown",
      openingClickSeen: false,
      pollTimer: null,
      controlBlurred: false,
      windowBlurred: false,
    };
    nativeControlGuard = current;
    current.pollTimer = window.setTimeout(
      () => pollNativeControl(current),
      NATIVE_POPUP_POLL_INTERVAL,
    );
  }

  function holdNativeControlGuard(event: Event) {
    const control = nativePopupControlFor(event.target);
    if (!control) {
      releaseNativeControlGuard();
      return;
    }
    acquireNativeControlGuard(control);
  }

  function releaseNativeControlGuardForEvent(event: Event) {
    const control = nativePopupControlFor(event.target);
    if (control) releaseNativeControlGuard(control);
  }

  function releaseNativeControlGuardOnBlur(event: FocusEvent) {
    const current = nativeControlGuard;
    if (current && event.target === current.control) {
      if (current.kind === "input-picker") {
        current.controlBlurred = true;
      } else {
        releaseNativeControlGuard(current.control);
      }
    }
  }

  function releaseInputPickerAfterFocusReturns() {
    const current = nativeControlGuard;
    if (current?.kind === "input-picker" && current.controlBlurred) {
      releaseNativeControlGuard(current.control);
    }
  }

  function handleNativeControlClick(event: MouseEvent) {
    const current = nativeControlGuard;
    const control = nativePopupControlFor(event.target);
    if (!current || control !== current.control) return;
    if (!current.openingClickSeen) {
      current.openingClickSeen = true;
    } else if (
      current.kind === "select" &&
      current.openSupport === "unsupported"
    ) {
      releaseNativeControlGuard(current.control);
    }
  }

  function handleNativeControlKey(event: KeyboardEvent) {
    const control = nativePopupControlFor(event.target);
    if (!control) return;
    const opensPopup =
      event.key === " " || (event.altKey && event.key === "ArrowDown");
    if (opensPopup) {
      acquireNativeControlGuard(control);
    } else if (event.key === "Escape" || event.key === "Enter") {
      releaseNativeControlGuard(control);
    }
  }

  function handleWindowBlur() {
    releasePointerGuard();
    const current = nativeControlGuard;
    if (!current) return;
    current.windowBlurred = true;
  }

  function handleWindowFocus() {
    const current = nativeControlGuard;
    if (current?.windowBlurred) {
      if (current.kind === "input-picker") {
        releaseNativeControlGuard(current.control);
      } else if (nativePopupIsOpen(current.control) !== true) {
        releaseNativeControlGuard(current.control);
      }
    }
    void queueNativeSync(true);
  }

  function handleVisibilityChange() {
    if (document.hidden) releasePointerGuard();
  }

  function releasePointerGuardAfterExit(event: PointerEvent) {
    if (event.relatedTarget == null) releasePointerGuardIfIdle(event);
  }

  function handleLostPointerCapture(event: PointerEvent) {
    if (Number.isInteger(event.pointerId)) {
      capturedPointers.delete(event.pointerId);
    }
    releasePointerGuardIfIdle(event);
  }

  document.addEventListener("pointerdown", holdPointerGuard, true);
  document.addEventListener("pointerdown", holdNativeControlGuard, true);
  document.addEventListener("keydown", handleNativeControlKey, true);
  document.addEventListener("click", handleNativeControlClick, true);
  document.addEventListener("change", releaseNativeControlGuardForEvent, true);
  document.addEventListener("input", releaseNativeControlGuardForEvent, true);
  document.addEventListener("blur", releaseNativeControlGuardOnBlur, true);
  document.addEventListener(
    "focusin",
    releaseInputPickerAfterFocusReturns,
    true,
  );
  document.addEventListener("pointerout", releasePointerGuardAfterExit, true);
  document.addEventListener(
    "lostpointercapture",
    handleLostPointerCapture,
    true,
  );
  document.addEventListener("visibilitychange", handleVisibilityChange);
  window.addEventListener("pointerup", releasePointerGuardIfIdle, true);
  window.addEventListener("pointercancel", releasePointerGuardIfIdle, true);
  window.addEventListener("pointermove", releasePointerGuardIfIdle, true);
  window.addEventListener("pointerover", releasePointerGuardIfIdle, true);
  window.addEventListener("dragend", releasePointerGuard, true);
  window.addEventListener("drop", releasePointerGuard, true);
  window.addEventListener("blur", handleWindowBlur);
  window.addEventListener("focus", handleWindowFocus);

  return () => {
    document.removeEventListener("pointerdown", holdPointerGuard, true);
    document.removeEventListener("pointerdown", holdNativeControlGuard, true);
    document.removeEventListener("keydown", handleNativeControlKey, true);
    document.removeEventListener("click", handleNativeControlClick, true);
    document.removeEventListener(
      "change",
      releaseNativeControlGuardForEvent,
      true,
    );
    document.removeEventListener(
      "input",
      releaseNativeControlGuardForEvent,
      true,
    );
    document.removeEventListener("blur", releaseNativeControlGuardOnBlur, true);
    document.removeEventListener(
      "focusin",
      releaseInputPickerAfterFocusReturns,
      true,
    );
    document.removeEventListener(
      "pointerout",
      releasePointerGuardAfterExit,
      true,
    );
    document.removeEventListener(
      "lostpointercapture",
      handleLostPointerCapture,
      true,
    );
    document.removeEventListener("visibilitychange", handleVisibilityChange);
    window.removeEventListener("pointerup", releasePointerGuardIfIdle, true);
    window.removeEventListener(
      "pointercancel",
      releasePointerGuardIfIdle,
      true,
    );
    window.removeEventListener("pointermove", releasePointerGuardIfIdle, true);
    window.removeEventListener("pointerover", releasePointerGuardIfIdle, true);
    window.removeEventListener("dragend", releasePointerGuard, true);
    window.removeEventListener("drop", releasePointerGuard, true);
    window.removeEventListener("blur", handleWindowBlur);
    window.removeEventListener("focus", handleWindowFocus);
    releasePointerGuard();
    releaseNativeControlGuard();
  };
}
