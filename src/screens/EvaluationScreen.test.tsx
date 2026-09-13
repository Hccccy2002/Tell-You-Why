import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { vi } from "vitest";
import { EvaluationScreen } from "./EvaluationScreen";
import * as api from "../lib/evaluation";
import { ragProviders } from "../lib/rag";
import type {
  EvaluationDocument,
  EvaluationJob,
  EvaluationReport,
} from "../lib/evaluation";

vi.mock("../lib/evaluation", () => ({
  evaluationList: vi.fn(),
  evaluationRead: vi.fn(),
  evaluationSaveReview: vi.fn(),
  evaluationStart: vi.fn(),
  evaluationCancel: vi.fn(),
  evaluationExport: vi.fn(),
}));
vi.mock("../lib/rag", () => ({ ragProviders: vi.fn() }));
vi.mock("../lib/autoHideGuard", () => ({ useAutoHideGuard: vi.fn() }));
const job: EvaluationJob = {
  id: "e1",
  kind: "review",
  title: "Agent 流程评测",
  status: "completed",
  created_at: "2026-09-13T00:00:00Z",
  finished_at: "2026-09-13T00:01:00Z",
  completed: 10,
  total: 10,
  message: "评测已完成",
  error: null,
  model: null,
};
const report: EvaluationReport = {
  title: job.title,
  dataset: "review-eval-v1",
  notes: ["模拟模型只验证流程"],
  metrics: [{ label: "流程通过", value: "9 / 10", baseline: null }],
  rows: [
    {
      id: "ok",
      title: "答对后的流程",
      status: "passed",
      note: "检查通过",
      details: { checks: { completed: true } },
    },
    {
      id: "bad",
      title: "失败恢复",
      status: "failed",
      note: "任务未完成",
      details: { checks: { completed: false } },
    },
  ],
};
beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(api.evaluationList).mockResolvedValue({
    jobs: [],
    unavailable_reason: null,
  });
  vi.mocked(api.evaluationRead).mockResolvedValue({ job, report });
  vi.mocked(api.evaluationStart).mockResolvedValue({
    ...job,
    status: "running",
    completed: 0,
  });
  vi.mocked(api.evaluationCancel).mockResolvedValue();
  vi.mocked(ragProviders).mockResolvedValue([
    { id: "deepseek", region: "default", model: "test" },
  ]);
});

it("opens without starting evaluations or requesting model providers", async () => {
  render(<EvaluationScreen />);
  await screen.findByText(/还没有评测记录/);
  expect(screen.getByRole("button", { name: "开始评测" })).toBeEnabled();
  expect(api.evaluationStart).not.toHaveBeenCalled();
  expect(ragProviders).not.toHaveBeenCalled();
});

it("starts one offline job and restores its background progress", async () => {
  const user = userEvent.setup();
  const running = {
    ...job,
    status: "running" as const,
    completed: 3,
    message: "正在评测第三题",
  };
  let finish!: (value: EvaluationJob) => void;
  vi.mocked(api.evaluationStart).mockImplementation(
    () =>
      new Promise((resolve) => {
        finish = resolve;
      }),
  );
  const view = render(<EvaluationScreen />);
  await screen.findByText(/还没有评测记录/);
  await user.dblClick(screen.getByRole("button", { name: "开始评测" }));
  expect(api.evaluationStart).toHaveBeenCalledExactlyOnceWith({
    kind: "review",
  });
  vi.mocked(api.evaluationList).mockResolvedValue({
    jobs: [running],
    unavailable_reason: null,
  });
  await act(async () => {
    finish(running);
    await Promise.resolve();
  });
  await screen.findByText("正在评测第三题");
  expect(screen.getByRole("progressbar", { name: "评测进度" })).toHaveAttribute(
    "value",
    "3",
  );
  expect(screen.getByRole("button", { name: "开始评测" })).toBeDisabled();
  view.unmount();
  render(<EvaluationScreen />);
  await screen.findByText("正在评测第三题");
  expect(api.evaluationStart).toHaveBeenCalledTimes(1);
  const cancelled = {
    ...running,
    status: "cancelled" as const,
    message: "评测已停止",
  };
  vi.mocked(api.evaluationList).mockResolvedValue({
    jobs: [cancelled],
    unavailable_reason: null,
  });
  await user.click(screen.getByRole("button", { name: "停止评测" }));
  await waitFor(() => expect(api.evaluationCancel).toHaveBeenCalledWith("e1"));
  await waitFor(() =>
    expect(screen.queryByLabelText("当前评测")).not.toBeInTheDocument(),
  );
});

it("shows real-model cost and sends the selected provider only on explicit start", async () => {
  const user = userEvent.setup();
  render(<EvaluationScreen />);
  await screen.findByText(/还没有评测记录/);
  await user.click(screen.getByRole("radio", { name: /真实模型/ }));
  await screen.findByRole("option", { name: /deepseek · test/ });
  expect(screen.getByText(/每轮最多 40 次请求/)).toBeVisible();
  expect(api.evaluationStart).not.toHaveBeenCalled();
  await user.click(screen.getByRole("button", { name: "开始真实模型评测" }));
  expect(api.evaluationStart).toHaveBeenCalledWith({
    kind: "model",
    provider: "deepseek",
    region: "default",
  });
});

it("does not launch real model evaluation without a configured provider", async () => {
  vi.mocked(ragProviders).mockResolvedValue([]);
  const user = userEvent.setup();
  render(<EvaluationScreen />);
  await user.click(screen.getByRole("radio", { name: /真实模型/ }));
  expect(
    await screen.findByRole("button", { name: "开始真实模型评测" }),
  ).toBeDisabled();
  expect(api.evaluationStart).not.toHaveBeenCalled();
});

it("shows metrics and failure details, filters results and exports the selected report", async () => {
  vi.mocked(api.evaluationList).mockResolvedValue({
    jobs: [job],
    unavailable_reason: null,
  });
  vi.mocked(api.evaluationExport).mockResolvedValue(
    "D:/reports/evaluation.json",
  );
  const user = userEvent.setup();
  render(<EvaluationScreen />);
  await screen.findByText("9 / 10");
  await user.selectOptions(screen.getByLabelText("逐项结果"), "failed");
  expect(screen.queryByText("答对后的流程")).not.toBeInTheDocument();
  await user.click(screen.getByText("失败恢复", { exact: true }));
  expect(screen.getByText("× 任务完成")).toBeVisible();
  await user.click(screen.getByRole("button", { name: "导出 JSON 报告" }));
  expect(api.evaluationExport).toHaveBeenCalledWith("e1");
  expect(await screen.findByRole("status")).toHaveTextContent(
    "D:/reports/evaluation.json",
  );
});

it("discards a late report after switching history entries", async () => {
  const older = { ...job, id: "e0", title: "历史评测" };
  vi.mocked(api.evaluationList).mockResolvedValue({
    jobs: [job, older],
    unavailable_reason: null,
  });
  let finish!: (value: EvaluationDocument) => void;
  vi.mocked(api.evaluationRead).mockImplementation((id) =>
    id === "e1"
      ? new Promise((resolve) => {
          finish = resolve;
        })
      : Promise.resolve({
          job: older,
          report: { ...report, dataset: "old-dataset" },
        }),
  );
  const user = userEvent.setup();
  render(<EvaluationScreen />);
  await screen.findByText("正在读取评测报告…");
  await user.selectOptions(
    await screen.findByLabelText("最近 50 次评测"),
    "e0",
  );
  await screen.findByText("数据集：old-dataset");
  await act(async () => {
    finish({ job, report });
    await Promise.resolve();
  });
  expect(screen.queryByText("数据集：review-eval-v1")).not.toBeInTheDocument();
  expect(
    within(screen.getByLabelText("评测记录")).getByRole("heading", {
      name: "历史评测",
    }),
  ).toBeVisible();
});

it("retains a launch error across a successful history refresh", async () => {
  vi.mocked(api.evaluationStart).mockRejectedValue(new Error("模型通道已变更"));
  const user = userEvent.setup();
  render(<EvaluationScreen />);
  await screen.findByText(/还没有评测记录/);
  await user.click(screen.getByRole("button", { name: "开始评测" }));
  await screen.findByText("模型通道已变更");
  await user.click(screen.getByRole("button", { name: "刷新" }));
  await waitFor(() => expect(api.evaluationList).toHaveBeenCalledTimes(2));
  expect(screen.getByRole("alert")).toHaveTextContent("模型通道已变更");
});

const modelJob: EvaluationJob = {
  ...job,
  id: "model-1",
  kind: "model",
  title: "真实模型评测",
};
const modelReport: EvaluationReport = {
  ...report,
  title: modelJob.title,
  metrics: [{ label: "事实正确率（人工）", value: "待复核", baseline: null }],
  human_review: {
    report_sha256: "report-fingerprint",
    reviewed: 0,
    total: 1,
    drafts: 0,
    error: null,
  },
  rows: [
    {
      id: "d01",
      title: "存储器的作用是什么？",
      status: "unreviewed",
      note: "内容待复核",
      human_review: { result_sha256: "result-fingerprint", annotation: null },
      details: {
        result: {
          draft: {
            status: "answered",
            question: "存储器的作用是什么？",
            reason: "",
            answer: [
              {
                text: "保存程序和数据。",
                citations: [{ evidence_id: "E1", quote: "存储程序和数据" }],
              },
            ],
            explanation: [],
          },
        },
        reference_answer: "存储器用于保存程序和数据。",
        evidence: [
          {
            id: "E1",
            block_id: "block",
            page: 5,
            chapter_path: ["第一章"],
            text: "存储器存储程序和数据。",
          },
        ],
      },
    },
  ],
};

function showModelReport() {
  let current: EvaluationDocument = structuredClone({
    job: modelJob,
    report: modelReport,
  });
  vi.mocked(api.evaluationList).mockResolvedValue({
    jobs: [modelJob],
    unavailable_reason: null,
  });
  vi.mocked(api.evaluationRead).mockImplementation(() =>
    Promise.resolve(structuredClone(current)),
  );
  vi.mocked(api.evaluationSaveReview).mockImplementation((request) => {
    const finished =
      request.review.correct !== null &&
      request.review.complete !== null &&
      request.review.grounded !== null &&
      !!request.review.notes.trim();
    current = structuredClone(current);
    const report = current.report!;
    report.rows[0]!.human_review!.annotation = {
      ...request.review,
      id: request.case_id,
      result_sha256: "result-fingerprint",
      method: finished ? "human" : null,
      revision: request.expected_revision + 1,
      updated_at: "2026-09-13T00:02:00Z",
    };
    report.human_review!.reviewed = finished ? 1 : 0;
    report.human_review!.drafts = finished ? 0 : 1;
    report.rows[0]!.status = !finished
      ? "unreviewed"
      : request.review.correct &&
          request.review.complete &&
          request.review.grounded
        ? "passed"
        : "failed";
    report.metrics[0]!.value = finished ? "0.0%" : "待复核";
    report.metrics[0]!.detail = finished ? "符合 0 / 1 道" : null;
    return Promise.resolve(structuredClone(current));
  });
}

async function openReview(user: ReturnType<typeof userEvent.setup>) {
  await user.click(await screen.findByText("存储器的作用是什么？"));
  return within(screen.getByRole("form", { name: "人工复核 d01" }));
}

it("saves a partial draft, restores it after reopening and scores only a completed review", async () => {
  showModelReport();
  const user = userEvent.setup();
  const view = render(<EvaluationScreen />);
  let form = await openReview(user);
  expect(form.getByLabelText("事实正确性")).toHaveValue("");
  await user.click(screen.getByText("相关原文（1 条）"));
  expect(screen.getByText("存储器存储程序和数据。")).toBeVisible();
  await user.selectOptions(form.getByLabelText("事实正确性"), "false");
  await user.click(form.getByRole("button", { name: "保存草稿" }));
  await screen.findByText("草稿已保存，尚未计入质量分数。");
  expect(screen.getByText("待复核", { exact: true })).toBeVisible();
  expect(api.evaluationSaveReview).toHaveBeenLastCalledWith(
    expect.objectContaining({
      id: "model-1",
      case_id: "d01",
      report_sha256: "report-fingerprint",
      expected_revision: 0,
      review: {
        reviewer: "本机用户",
        correct: false,
        complete: null,
        grounded: null,
        notes: "",
      },
    }),
  );
  view.unmount();
  render(<EvaluationScreen />);
  form = await openReview(user);
  expect(form.getByLabelText("事实正确性")).toHaveValue("false");
  await user.selectOptions(form.getByLabelText("回答完整性"), "true");
  await user.selectOptions(form.getByLabelText("证据支持情况"), "true");
  await user.type(
    form.getByLabelText("复核备注"),
    "测试复核：关键事实需要修正。",
  );
  await user.click(form.getByRole("button", { name: "保存复核" }));
  await screen.findByText("复核已保存，质量分数已更新。");
  expect(screen.getByText("0.0%")).toBeVisible();
  expect(screen.getByText("符合 0 / 1 道")).toBeVisible();
  expect(api.evaluationSaveReview).toHaveBeenLastCalledWith(
    expect.objectContaining({ expected_revision: 1 }),
  );
  await user.selectOptions(screen.getByLabelText("逐项结果"), "failed");
  expect(screen.getByText("存储器的作用是什么？")).toBeVisible();
  await user.selectOptions(screen.getByLabelText("逐项结果"), "unreviewed");
  expect(screen.queryByText("存储器的作用是什么？")).not.toBeInTheDocument();
  expect(api.evaluationStart).not.toHaveBeenCalled();
});

it("keeps edited judgments through filtering, refresh and a failed save", async () => {
  showModelReport();
  vi.mocked(api.evaluationSaveReview).mockRejectedValueOnce(
    new Error("磁盘写入失败"),
  );
  const user = userEvent.setup();
  render(<EvaluationScreen />);
  let form = await openReview(user);
  await user.type(form.getByLabelText("复核备注"), "先保留这条判断依据");
  await user.selectOptions(screen.getByLabelText("逐项结果"), "passed");
  await user.selectOptions(screen.getByLabelText("逐项结果"), "all");
  form = await openReview(user);
  expect(form.getByLabelText("复核备注")).toHaveValue("先保留这条判断依据");
  await user.click(screen.getByRole("button", { name: "刷新" }));
  await user.click(form.getByRole("button", { name: "保存草稿" }));
  await screen.findByText(/磁盘写入失败/);
  expect(form.getByLabelText("复核备注")).toHaveValue("先保留这条判断依据");
  expect(form.getByRole("button", { name: "保存草稿" })).toBeEnabled();
  await user.click(form.getByRole("button", { name: "保存草稿" }));
  await screen.findByText("草稿已保存，尚未计入质量分数。");
});

it("does not let a delayed review save replace a different history report", async () => {
  showModelReport();
  vi.mocked(api.evaluationList).mockResolvedValue({
    jobs: [modelJob, job],
    unavailable_reason: null,
  });
  vi.mocked(api.evaluationRead).mockImplementation((id) =>
    Promise.resolve(
      id === job.id ? { job, report } : { job: modelJob, report: modelReport },
    ),
  );
  let finish!: (doc: EvaluationDocument) => void;
  vi.mocked(api.evaluationSaveReview).mockImplementation(
    () =>
      new Promise((resolve) => {
        finish = resolve;
      }),
  );
  const user = userEvent.setup();
  render(<EvaluationScreen />);
  const form = await openReview(user);
  await user.selectOptions(form.getByLabelText("事实正确性"), "true");
  await user.dblClick(form.getByRole("button", { name: "保存草稿" }));
  expect(api.evaluationSaveReview).toHaveBeenCalledTimes(1);
  await user.selectOptions(screen.getByLabelText("最近 50 次评测"), job.id);
  await screen.findByText("9 / 10");
  await act(async () => {
    finish({ job: modelJob, report: modelReport });
    await Promise.resolve();
  });
  expect(screen.getByText("9 / 10")).toBeVisible();
  expect(screen.queryByText("存储器的作用是什么？")).not.toBeInTheDocument();
});

it("disables review of a report whose annotations no longer match", async () => {
  showModelReport();
  vi.mocked(api.evaluationRead).mockResolvedValue({
    job: modelJob,
    report: {
      ...modelReport,
      human_review: { error: "报告内容已变化，已有复核不再适用" },
    },
  });
  const user = userEvent.setup();
  render(<EvaluationScreen />);
  await screen.findByText("报告内容已变化，已有复核不再适用");
  await user.click(screen.getByText("存储器的作用是什么？"));
  expect(screen.getByText("保存程序和数据。")).toBeVisible();
  expect(
    screen.queryByRole("form", { name: "人工复核 d01" }),
  ).not.toBeInTheDocument();
  expect(api.evaluationSaveReview).not.toHaveBeenCalled();
});

it("preserves a conflicting local draft until explicitly loading the newer review", async () => {
  showModelReport();
  const user = userEvent.setup();
  render(<EvaluationScreen />);
  const form = await openReview(user);
  await user.type(form.getByLabelText("复核备注"), "尚未保存的本地判断");
  const updated = structuredClone(modelReport);
  updated.rows[0]!.human_review!.annotation = {
    id: "d01",
    result_sha256: "result-fingerprint",
    method: "human",
    revision: 3,
    updated_at: "2026-09-13T01:00:00Z",
    reviewer: "另一位复核人",
    correct: false,
    complete: true,
    grounded: false,
    notes: "已经保存的新判断",
  };
  vi.mocked(api.evaluationRead).mockResolvedValue({
    job: modelJob,
    report: updated,
  });
  await user.click(screen.getByRole("button", { name: "刷新" }));
  await screen.findByRole("button", { name: "放弃修改并读取已保存版本" });
  expect(form.getByLabelText("复核备注")).toHaveValue("尚未保存的本地判断");
  await user.click(
    form.getByRole("button", { name: "放弃修改并读取已保存版本" }),
  );
  expect(form.getByLabelText("复核备注")).toHaveValue("已经保存的新判断");
  expect(form.getByLabelText("复核人")).toHaveValue("另一位复核人");
  await user.type(form.getByLabelText("复核备注"), "，补充证据");
  await user.click(form.getByRole("button", { name: "保存复核" }));
  expect(api.evaluationSaveReview).toHaveBeenLastCalledWith(
    expect.objectContaining({ expected_revision: 3 }),
  );
});
