import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { vi } from "vitest";
import { BenchmarkPanel } from "./BenchmarkPanel";
import { evaluationBenchmark, type BenchmarkDocument } from "../lib/benchmark";
import type * as BenchmarkApi from "../lib/benchmark";

vi.mock("../lib/benchmark", async (original) => ({
  ...(await original<typeof BenchmarkApi>()),
  evaluationBenchmark: vi.fn(),
}));
vi.mock("../lib/knowledgeBase", () => ({ kbRead: vi.fn() }));

const fixture: BenchmarkDocument = {
  revision: 2,
  dataset: {
    kb: "book",
    knowledge_version: "v1",
    source_filename: "测试教材.pdf",
    source_pages: 10,
    cases: [
      {
        id: "q1",
        question: "测试问题",
        reference_answer: "参考答案",
        acceptance_criteria: "关键事实准确",
        split: "regression",
        kind: "single",
        expected_behavior: "answer",
        evidence: [],
        origin: "legacy_exposed",
        review: {
          method: "unreviewed",
          human_reviewer: null,
          date: null,
          notes: "",
        },
      },
    ],
  },
  releases: [
    {
      id: "release",
      version: "human-test",
      created_at: "2026-09-16",
      cases: [{ id: "held1", question: "留出问题", split: "holdout" }],
    },
  ],
  uses: [],
};
beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(evaluationBenchmark).mockResolvedValue(structuredClone(fixture));
});

it("loads on demand and never preselects a human declaration", async () => {
  const user = userEvent.setup();
  render(<BenchmarkPanel kind="top5" onChange={vi.fn()} />);
  expect(evaluationBenchmark).not.toHaveBeenCalled();
  await user.click(screen.getByText("管理人工评测题库"));
  expect(await screen.findByLabelText("问题")).toHaveValue("测试问题");
  expect(screen.getByLabelText(/我已人工核对/)).not.toBeChecked();
  expect(screen.getByRole("button", { name: "封存已确认题目" })).toBeDisabled();
  expect(screen.getByRole("option", { name: "留出测试集" })).toBeDisabled();
});

it("saves an attributed declaration and clears confirmation after editing", async () => {
  const user = userEvent.setup();
  render(<BenchmarkPanel kind="model" onChange={vi.fn()} />);
  await user.click(screen.getByText("管理人工评测题库"));
  await user.type(await screen.findByLabelText("标注复核人"), "测试署名");
  await user.type(screen.getByLabelText("标注核对依据"), "测试用核对记录");
  await user.click(screen.getByLabelText(/我已人工核对/));
  await user.type(screen.getByLabelText("参考答案"), "修改");
  expect(screen.getByLabelText(/我已人工核对/)).not.toBeChecked();
  expect(screen.getByLabelText("选择标注题目")).toBeDisabled();
  await user.click(screen.getByLabelText(/我已人工核对/));
  await user.click(screen.getByRole("button", { name: "保存并确认标注" }));
  await waitFor(() =>
    expect(vi.mocked(evaluationBenchmark).mock.calls.at(-1)?.[0]).toMatchObject(
      {
        action: "save",
        revision: 2,
        confirm_human: true,
        case: {
          review: {
            human_reviewer: "测试署名",
            notes: "测试用核对记录",
          },
          reference_answer: "参考答案修改",
        },
      },
    ),
  );
});

it("requires explicit holdout use and preserves input on save failures", async () => {
  const user = userEvent.setup();
  const onChange = vi.fn();
  render(<BenchmarkPanel kind="model" onChange={onChange} />);
  await user.click(screen.getByText("管理人工评测题库"));
  await user.click(await screen.findByLabelText("本轮使用人工标注基准"));
  await user.selectOptions(screen.getByLabelText("封存版本"), "release");
  await user.selectOptions(screen.getByLabelText("评测分组"), "holdout");
  await waitFor(() =>
    expect(onChange).toHaveBeenLastCalledWith(
      expect.objectContaining({ valid: false }),
    ),
  );
  await user.click(screen.getByLabelText(/方案已固定/));
  await waitFor(() =>
    expect(onChange).toHaveBeenLastCalledWith({
      valid: true,
      selection: {
        release: "release",
        split: "holdout",
        case_ids: ["held1"],
        acknowledge_holdout: true,
      },
    }),
  );
  await user.type(screen.getByLabelText("参考答案"), "保留输入");
  vi.mocked(evaluationBenchmark).mockRejectedValueOnce(
    new Error("题库已被修改"),
  );
  await user.click(screen.getByRole("button", { name: "保存题目草稿" }));
  await screen.findByRole("alert");
  expect(screen.getByLabelText("参考答案")).toHaveValue("参考答案保留输入");
});
