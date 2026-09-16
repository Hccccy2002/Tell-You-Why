import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { vi } from "vitest";
import { CardStudyActivity } from "./CardStudyActivity";

it("opens the saved session by ID and ignores a late response for another card", async () => {
  let old!: (items: []) => void;
  const client = {
    cardSessions: vi
      .fn()
      .mockImplementationOnce(
        () =>
          new Promise((r) => {
            old = r;
          }),
      )
      .mockResolvedValue([
        {
          id: "new-session",
          goal: "新卡",
          state: "completed",
          step_count: 2,
          created_at: "2026-09-16",
        },
      ]),
  };
  const onOpen = vi.fn();
  const view = render(
    <CardStudyActivity
      cardId="old"
      onOpen={onOpen}
      disabled={false}
      client={client}
    />,
  );
  view.rerender(
    <CardStudyActivity
      cardId="new"
      onOpen={onOpen}
      disabled={false}
      client={client}
    />,
  );
  await userEvent
    .setup()
    .click(await screen.findByRole("button", { name: /回看上次学习/ }));
  expect(onOpen).toHaveBeenCalledWith("new-session");
  old([]);
  expect(screen.getByRole("button", { name: /回看上次学习/ })).toBeVisible();
});
