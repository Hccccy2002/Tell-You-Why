import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { isDesktop } from "./api";
export const takeWindowNavigation = <T>() =>
  invoke<T | null>("take_window_navigation");
export function listenWindowNavigation<T>(
  onNavigate: (entry: T, id: string) => boolean,
  onError: (error: unknown) => void,
) {
  if (!isDesktop()) return () => {};
  let active = true;
  let receiving = false;
  let requested = false;
  async function receive() {
    requested = true;
    if (receiving) return;
    receiving = true;
    try {
      while (active && requested) {
        requested = false;
        const request = await takeWindowNavigation<{ id: string; entry: T }>();
        // A busy or unmounted recipient leaves the request available for reopening.
        if (active && request && onNavigate(request.entry, request.id)) {
          await invoke("acknowledge_window_navigation", { id: request.id });
        }
      }
    } catch (error) {
      if (active) onError(error);
    } finally {
      receiving = false;
    }
  }
  const subscription = listen("window-navigation", () => void receive());
  void subscription.then(() => active && void receive()).catch(onError);
  return () => {
    active = false;
    void subscription.then((stop) => stop()).catch(() => {});
  };
}
