import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { vi } from "vitest";
import { StudyCenter } from "./StudyCenter";
import { studyClient } from "../lib/study";

const review = {
  concept_key: "dns",
  topic: "网络",
  title: "DNS",
  attempts: 1,
  last_correct: false,
  due_at: 1,
};
const doubt = {
  id: "doubt",
  question: "生成和挥发有什么区别？",
  topic: "科学",
  session_id: "previous",
};
function show() {
  const client = {
    ...studyClient,
    home: vi.fn().mockResolvedValue({
      active: null,
      goals: [{ id: "goal", title: "分清变化" }],
      due: [review],
      due_count: 1,
      practice_count: 1,
      next_due_at: null,
      personalization_enabled: true,
      doubts: [doubt],
    }),
    latest: vi.fn().mockResolvedValue(null),
    read: vi.fn().mockResolvedValue(null),
    continue: vi.fn(),
    history: vi.fn().mockResolvedValue([]),
  };
  render(
    <StudyCenter
      topics={[]}
      providers={[]}
      client={client}
      onModelSettings={vi.fn()}
      onOpenCard={vi.fn()}
    />,
  );
  return client;
}
it("opens an exact saved goal and returns to the learning homepage", async () => {
  const client = show();
  await userEvent.click(
    await screen.findByRole("button", { name: "分清变化" }),
  );
  expect(client.read).toHaveBeenCalledWith("goal");
  await userEvent.click(
    await screen.findByRole("button", { name: "← 返回学习首页" }),
  );
  expect(
    await screen.findByRole("heading", { name: "接着学一点" }),
  ).toBeVisible();
  expect(client.continue).not.toHaveBeenCalled();
});
it.each([
  ["DNS", "巩固准备"],
  [doubt.question, "继续疑问预览"],
])("opens %s without requesting a model", async (title, region) => {
  const client = show();
  await userEvent.click(await screen.findByRole("button", { name: title }));
  expect(await screen.findByRole("region", { name: region })).toBeVisible();
  expect(client.continue).not.toHaveBeenCalled();
});
