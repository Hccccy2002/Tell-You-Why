import { act, renderHook, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { vi } from "vitest";
import { useModelBusy } from "./windowSync";
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("./api", () => ({ isDesktop: () => true }));
it("reflects the native shared model lock and refreshes on focus", async () => {
  vi.mocked(invoke).mockResolvedValue(true);
  const { result, unmount } = renderHook(() => useModelBusy());
  await waitFor(() => expect(result.current).toBe(true));
  expect(invoke).toHaveBeenCalledWith("model_operation_busy");
  vi.mocked(invoke).mockResolvedValue(false);
  act(() => {
    window.dispatchEvent(new Event("focus"));
  });
  await waitFor(() => expect(result.current).toBe(false));
  unmount();
  vi.mocked(invoke).mockClear();
  window.dispatchEvent(new Event("focus"));
  expect(invoke).not.toHaveBeenCalled();
});
