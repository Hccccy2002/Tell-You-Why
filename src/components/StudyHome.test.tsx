import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { vi } from "vitest";
import { StudyHome } from "./StudyHome";
import type { StudyHomeData, StudyDueItem } from "../lib/study";

const due: StudyDueItem = {
  concept_key: "network:dns",
  topic: "网络",
  title: "DNS 的作用",
  attempts: 1,
  last_correct: false,
  due_at: 1,
};
function overview(overrides: Partial<StudyHomeData> = {}): StudyHomeData {
  return {
    active: null,
    due: [due],
    due_count: 1,
    practice_count: 1,
    next_due_at: null,
    personalization_enabled: true,
    ...overrides,
  };
}

it("recommends only explicit unresolved questions and hides them when personalization is off", async () => {
  const question = {
    id: "doubt",
    question: "为什么名字不变地址会变？",
    topic: "网络",
    session_id: "old",
  };
  const onDoubt = vi.fn();
  const client = {
    home: vi.fn().mockResolvedValue(overview({ doubts: [question] })),
  };
  const view = render(
    <StudyHome
      busy={false}
      onOpen={vi.fn()}
      onDoubt={onDoubt}
      client={client}
    />,
  );
  await userEvent.click(
    await screen.findByRole("button", { name: /为什么名字不变地址会变/ }),
  );
  expect(onDoubt).toHaveBeenCalledWith(question);
  expect(
    screen.getByText("你上次反馈“还没懂”，可以换一种方式接着讲"),
  ).toBeVisible();
  view.rerender(
    <StudyHome
      busy={false}
      onOpen={vi.fn()}
      onDoubt={onDoubt}
      client={{
        home: vi
          .fn()
          .mockResolvedValue(
            overview({ personalization_enabled: false, doubts: [question] }),
          ),
      }}
    />,
  );
  await screen.findByRole("button", { name: /陪我学一会儿/ });
  await waitFor(() =>
    expect(
      screen.queryByRole("region", { name: "接着解决疑问" }),
    ).not.toBeInTheDocument(),
  );
});

it("explains why a submitted practice is due and only opens its preview", async () => {
  const onReview = vi.fn();
  const home = vi.fn().mockResolvedValue(overview());
  render(
    <StudyHome
      busy={false}
      onOpen={vi.fn()}
      onReview={onReview}
      client={{ home }}
    />,
  );
  await userEvent.click(
    await screen.findByRole("button", { name: /DNS 的作用/ }),
  );
  expect(onReview).toHaveBeenCalledWith(due);
  expect(
    screen.getByText("上次答案与参考答案不一致，换个例子再看看"),
  ).toBeVisible();
  expect(home).toHaveBeenCalledTimes(1);
});

it("keeps an unfinished session ahead of starting consolidation", async () => {
  const active: StudyHomeData["active"] = {
    id: "pending",
    goal: "网络",
    topic: "网络",
    state: "paused",
    step_count: 1,
    last_title: "DNS",
    updated_at: "2026-09-16",
  };
  render(
    <StudyHome
      busy={false}
      onOpen={vi.fn()}
      onReview={vi.fn()}
      client={{ home: vi.fn().mockResolvedValue(overview({ active })) }}
    />,
  );
  expect(
    await screen.findByRole("button", { name: /DNS 的作用/ }),
  ).toBeDisabled();
  expect(screen.getByText("先继续或结束上次学习，再开始巩固。")).toBeVisible();
});

it("shows honest empty and privacy states instead of inventing due practice", async () => {
  const home = vi
    .fn()
    .mockResolvedValue(overview({ due: [], due_count: 0, practice_count: 0 }));
  const view = render(
    <StudyHome
      busy={false}
      onOpen={vi.fn()}
      onReview={vi.fn()}
      client={{ home }}
    />,
  );
  expect(await screen.findByText(/做完一道可选练习后/)).toBeVisible();
  view.unmount();
  render(
    <StudyHome
      busy={false}
      onOpen={vi.fn()}
      onReview={vi.fn()}
      client={{
        home: vi
          .fn()
          .mockResolvedValue(
            overview({ due: [], due_count: 0, personalization_enabled: false }),
          ),
      }}
    />,
  );
  expect(await screen.findByText(/个性化已关闭/)).toBeVisible();
  expect(
    screen.queryByRole("button", { name: /巩固这个知识点/ }),
  ).not.toBeInTheDocument();
});

it("shows the saved goal and opens exactly that session using only local overview data", async () => {
  const onOpen = vi.fn();
  const home = vi.fn().mockResolvedValue({
    active: {
      id: "saved",
      goal: "了解 DNS",
      topic: "网络",
      state: "waiting",
      step_count: 2,
      last_title: "电话号码簿",
      updated_at: "2026-09-16",
    },
  });
  render(<StudyHome busy={false} onOpen={onOpen} client={{ home }} />);
  const button = await screen.findByRole("button", { name: /继续上次/ });
  expect(button).toHaveTextContent("了解 DNS");
  expect(button).toHaveTextContent("已留下 2 步");
  await userEvent.click(button);
  expect(onOpen).toHaveBeenCalledWith("saved");
  expect(home).toHaveBeenCalledTimes(1);
});

it("offers a short learning session when there is no saved session", async () => {
  const onOpen = vi.fn();
  render(
    <StudyHome
      busy={false}
      onOpen={onOpen}
      client={{ home: vi.fn().mockResolvedValue({ active: null }) }}
    />,
  );
  await userEvent.click(
    await screen.findByRole("button", { name: /陪我学一会儿/ }),
  );
  expect(onOpen).toHaveBeenCalledWith(undefined);
  expect(screen.queryByText("继续上次")).not.toBeInTheDocument();
});

it("lets the reader retry a failed local load", async () => {
  const home = vi
    .fn()
    .mockRejectedValueOnce(Error("read failed"))
    .mockResolvedValue({ active: null });
  render(<StudyHome busy={false} onOpen={vi.fn()} client={{ home }} />);
  await userEvent.click(
    await screen.findByRole("button", { name: "重试读取" }),
  );
  await screen.findByRole("button", { name: /陪我学一会儿/ });
  expect(home).toHaveBeenCalledTimes(2);
});
