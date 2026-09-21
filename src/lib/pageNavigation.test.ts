import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { waitFor } from "@testing-library/react";
import { vi } from "vitest";
import { listenWindowNavigation } from "./pageNavigation";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));
vi.mock("./api", () => ({ isDesktop: () => true }));
const ipc = vi.mocked(invoke);
beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(listen).mockResolvedValue(vi.fn());
  ipc.mockResolvedValue(null);
});
it("receives a request queued before startup and acknowledges only after delivery", async () => {
  ipc.mockResolvedValueOnce({ id: "request", entry: { kind: "study" } });
  const handler = vi.fn().mockReturnValue(true);
  const stop = listenWindowNavigation(handler, vi.fn());
  await waitFor(() =>
    expect(handler).toHaveBeenCalledWith({ kind: "study" }, "request"),
  );
  expect(ipc).toHaveBeenLastCalledWith("acknowledge_window_navigation", {
    id: "request",
  });
  stop();
});
it("retains navigation when the receiving window is busy", async () => {
  ipc.mockResolvedValue({ id: "request", entry: { kind: "study" } });
  const handler = vi.fn().mockReturnValue(false);
  const stop = listenWindowNavigation(handler, vi.fn());
  await waitFor(() => expect(handler).toHaveBeenCalledOnce());
  expect(ipc).not.toHaveBeenCalledWith(
    "acknowledge_window_navigation",
    expect.anything(),
  );
  stop();
});
it("does not consume an in-flight request after its recipient unmounts", async () => {
  let finish!: (value: unknown) => void;
  ipc.mockImplementation(
    () =>
      new Promise((resolve) => {
        finish = resolve;
      }),
  );
  const handler = vi.fn().mockReturnValue(true);
  const stop = listenWindowNavigation(handler, vi.fn());
  await waitFor(() =>
    expect(ipc).toHaveBeenCalledWith("take_window_navigation"),
  );
  stop();
  finish({ id: "request", entry: { kind: "study" } });
  await Promise.resolve();
  expect(handler).not.toHaveBeenCalled();
  expect(ipc).not.toHaveBeenCalledWith(
    "acknowledge_window_navigation",
    expect.anything(),
  );
});
