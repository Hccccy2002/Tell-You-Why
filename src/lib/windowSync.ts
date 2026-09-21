import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { emit, listen } from "@tauri-apps/api/event";
import { isDesktop } from "./api";

export type SharedChange =
  "settings" | "study" | "study-reset" | "all" | "preferences" | "history";

export function notifySharedData(scope: SharedChange) {
  if (isDesktop()) void emit("shared-data-changed", scope).catch(() => {});
}

export function listenSharedData(onChange: (scope: SharedChange) => void) {
  if (!isDesktop()) return () => {};
  let active = true;
  const subscription = listen<SharedChange>(
    "shared-data-changed",
    ({ payload }) => {
      if (active) onChange(payload);
    },
  );
  void subscription.catch(() => {});
  return () => {
    active = false;
    void subscription.then((stop) => stop()).catch(() => {});
  };
}

// The native GenerationLock remains authoritative, including requests finishing after a page switch.
export function useModelBusy() {
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    if (!isDesktop()) return;
    let active = true;
    let polling = false;
    const refresh = async () => {
      if (!active || polling || document.hidden) return;
      polling = true;
      try {
        const value = await invoke<boolean>("model_operation_busy");
        if (active) setBusy(value);
      } catch {
        // The command that actually starts work still checks the native lock.
      } finally {
        polling = false;
      }
    };
    const onRefresh = () => void refresh();
    onRefresh();
    const timer = window.setInterval(onRefresh, 1000);
    window.addEventListener("focus", onRefresh);
    document.addEventListener("visibilitychange", onRefresh);
    return () => {
      active = false;
      window.clearInterval(timer);
      window.removeEventListener("focus", onRefresh);
      document.removeEventListener("visibilitychange", onRefresh);
    };
  }, []);
  return busy;
}
