import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { vi } from "vitest";
import { RelatedSources } from "./RelatedSources";
import { ragRelatedSources, type EvidencePacket } from "../lib/rag";

vi.mock("../lib/rag", () => ({ ragRelatedSources: vi.fn() }));
const request = {
  kb: "book",
  version: "v1",
  chapter: "c1",
  query: "为什么需要 Cache？",
  source_sha256: "source",
};
const packet: EvidencePacket = {
  ...request,
  filename: "教材.pdf",
  source_sha256: "source",
  sha256: "packet",
  text_chars: 50,
  status: "candidates",
  evidence: Array.from({ length: 9 }, (_, index) => ({
    id: `E${index + 1}`,
    block_id: `block-${index}`,
    text: `相关内容 ${index + 1}`,
    page: index + 2,
    chapter_path: ["存储系统"],
  })),
};
beforeEach(() => vi.clearAllMocks());

it("retrieves only on expansion, shows five in backend rank order and reuses the result", async () => {
  vi.mocked(ragRelatedSources).mockResolvedValue({
    ...packet,
    evidence: [packet.evidence[0]!, ...packet.evidence],
  });
  const onPage = vi.fn();
  const props = { request, active: false, onPage };
  const mounted = render(<RelatedSources {...props} />);
  expect(ragRelatedSources).not.toHaveBeenCalled();
  mounted.rerender(<RelatedSources {...props} active />);
  await screen.findByText("按与本题的相关度排序 · 5 段原文");
  expect(
    [...mounted.container.querySelectorAll("blockquote")].map(
      (e) => e.textContent,
    ),
  ).toEqual([
    "相关内容 1",
    "相关内容 2",
    "相关内容 3",
    "相关内容 4",
    "相关内容 5",
  ]);
  expect(ragRelatedSources).toHaveBeenCalledExactlyOnceWith(request);
  const user = userEvent.setup();
  await user.click(screen.getByRole("button", { name: "查看第 2 页原文 ↗" }));
  expect(onPage).toHaveBeenCalledWith(2, "v1");
  mounted.rerender(<RelatedSources {...props} />);
  mounted.rerender(<RelatedSources {...props} active />);
  await screen.findByText("相关内容 1");
  expect(ragRelatedSources).toHaveBeenCalledTimes(1);
});

it("discards late results for another book or question", async () => {
  let resolveOld!: (value: EvidencePacket) => void;
  vi.mocked(ragRelatedSources).mockImplementationOnce(
    () =>
      new Promise((resolve) => {
        resolveOld = resolve;
      }),
  );
  const mounted = render(
    <RelatedSources request={request} active onPage={vi.fn()} />,
  );
  expect(screen.getByRole("status")).toHaveTextContent("正在查找");
  const newer = {
    ...request,
    kb: "other-book",
    version: "v2",
    query: "另一题",
  };
  vi.mocked(ragRelatedSources).mockResolvedValue({
    ...packet,
    ...newer,
    evidence: [{ ...packet.evidence[0]!, text: "新题原文" }],
  });
  mounted.rerender(<RelatedSources request={newer} active onPage={vi.fn()} />);
  await screen.findByText("新题原文");
  await act(async () => {
    resolveOld(packet);
    await Promise.resolve();
  });
  expect(screen.queryByText("相关内容 1")).not.toBeInTheDocument();
  expect(screen.getByText("新题原文")).toBeInTheDocument();
});

it("allows retry after retrieval failure and never invents five sources for an empty result", async () => {
  vi.mocked(ragRelatedSources).mockRejectedValueOnce(new Error("模型未准备"));
  render(<RelatedSources request={request} active onPage={vi.fn()} />);
  expect(await screen.findByRole("alert")).toHaveTextContent("模型未准备");
  expect(screen.queryByRole("blockquote")).not.toBeInTheDocument();
  vi.mocked(ragRelatedSources).mockResolvedValue({ ...packet, evidence: [] });
  await userEvent
    .setup()
    .click(screen.getByRole("button", { name: "重新检索" }));
  await screen.findByText("暂未找到可用的相关原文。");
  expect(ragRelatedSources).toHaveBeenCalledTimes(2);
  expect(
    screen.queryByRole("button", { name: /查看第/ }),
  ).not.toBeInTheDocument();
});
