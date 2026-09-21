import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { vi } from "vitest";
import type * as Api from "../lib/api";
import { openSourceUrl } from "../lib/api";
import type { SearchAnswer } from "../types";
import { SearchAnswerView } from "./SearchAnswerView";

vi.mock("../lib/api", async (original) => ({
  ...(await original<typeof Api>()),
  openSourceUrl: vi.fn().mockResolvedValue(undefined),
}));

const answer: SearchAnswer = {
  runId: "saved-run",
  status: "partial",
  asOf: "2026-09-21",
  retrievedAt: "2026-09-21T03:00:00Z",
  cacheHit: false,
  blocks: [{ text: "资料支持部分结论。", evidenceIds: ["W1"] }],
  limitation: "无法确认当前最新状态。",
  sources: [
    {
      id: "W1",
      title: "协议说明",
      url: "https://example.org/protocol",
      snippet: "公开资料摘要",
      publisher: null,
      publishedAt: null,
      retrievedAt: "2026-09-21T03:00:00Z",
    },
  ],
};

it("shows only source titles and opens the persisted source binding", async () => {
  const user = userEvent.setup();
  render(<SearchAnswerView answer={answer} />);
  expect(screen.getByText(/资料仅支持部分回答/)).toBeInTheDocument();
  expect(screen.getByText("无法确认当前最新状态。")).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "查看来源 W1" }));
  expect(screen.getByRole("link", { name: "协议说明" })).toHaveAttribute(
    "href",
    "https://example.org/protocol",
  );
  expect(screen.queryByText("公开资料摘要")).not.toBeInTheDocument();
  expect(screen.queryByText(/发布：|检索于/)).not.toBeInTheDocument();
  await user.click(screen.getByRole("link", { name: "协议说明" }));
  expect(openSourceUrl).toHaveBeenLastCalledWith(
    "https://example.org/protocol",
  );
  await user.click(screen.getByRole("button", { name: /参考资料/ }));
  expect(
    screen.queryByRole("link", { name: "协议说明" }),
  ).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: /参考资料/ }));
  expect(screen.getByRole("link", { name: "协议说明" })).toBeVisible();
});

it("shows source opening failures without losing the saved answer", async () => {
  vi.mocked(openSourceUrl).mockRejectedValueOnce(new Error("无法打开来源"));
  const user = userEvent.setup();
  render(<SearchAnswerView answer={answer} />);
  await user.click(screen.getByRole("button", { name: "查看来源 W1" }));
  await user.click(screen.getByRole("link", { name: "协议说明" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("无法打开来源");
  expect(screen.getByText("资料支持部分结论。")).toBeInTheDocument();
});

it("keeps an unlinked source title without inventing a link or displaying its snippet", async () => {
  vi.mocked(openSourceUrl).mockClear();
  const user = userEvent.setup();
  render(
    <SearchAnswerView
      answer={{
        ...answer,
        sources: answer.sources.map((source) => ({ ...source, url: null })),
      }}
    />,
  );
  await user.click(screen.getByRole("button", { name: "查看来源 W1" }));
  expect(screen.getByText("协议说明")).toBeVisible();
  expect(screen.getByTitle("原文链接不可用")).toBeVisible();
  expect(screen.queryByText("公开资料摘要")).not.toBeInTheDocument();
  expect(screen.queryByText(/发布：|检索于/)).not.toBeInTheDocument();
  expect(
    screen.queryByRole("link", { name: "协议说明" }),
  ).not.toBeInTheDocument();
  expect(openSourceUrl).not.toHaveBeenCalled();
});
