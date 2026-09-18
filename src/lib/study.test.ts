import { invoke } from "@tauri-apps/api/core";
import { vi } from "vitest";
import { studyClient } from "./study";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

it("uses explicit goal commands and preserves an empty checkin as a skip", async () => {
  await studyClient.startGoal("分清两个概念");
  expect(invoke).toHaveBeenLastCalledWith("study_start_goal", {
    goal: "分清两个概念",
  });
  await studyClient.goalCheckin("saved", "");
  expect(invoke).toHaveBeenLastCalledWith("study_goal_checkin", {
    id: "saved",
    reply: "",
  });
  await studyClient.reopenGoal("saved");
  expect(invoke).toHaveBeenLastCalledWith("study_reopen_goal", { id: "saved" });
});

it("sends a clarification reply with its exact parent question to the desktop", async () => {
  await studyClient.ask("session", "step", "卡在地址变化", "request", "parent");
  expect(invoke).toHaveBeenLastCalledWith("study_ask", {
    id: "session",
    stepId: "step",
    question: "卡在地址变化",
    requestId: "request",
    replyToQuestionId: "parent",
  });
});

it("keeps ordinary questions separate from clarification replies", async () => {
  await studyClient.ask("session", "step", "另一个问题", "request");
  expect(invoke).toHaveBeenLastCalledWith("study_ask", {
    id: "session",
    stepId: "step",
    question: "另一个问题",
    requestId: "request",
    replyToQuestionId: null,
  });
});
