import { mkdir, writeFile } from "node:fs/promises";
import { randomUUID } from "node:crypto";
import { performance } from "node:perf_hooks";
import { fileURLToPath } from "node:url";
import {
  buildRequest,
  classifyResponse,
  endpoint,
  readBoundedJson,
  sanitizeResponse,
} from "./contract.mjs";

const apiKey = process.env.ZHIPU_API_KEY?.trim();
if (!apiKey || /\s/u.test(apiKey) || apiKey.length < 8) {
  console.error("ZHIPU_API_KEY 未配置或格式不完整；未发送请求。");
  process.exitCode = 2;
} else {
  const outputDir = new URL("../../tmp/search-agent/", import.meta.url);
  await mkdir(outputDir, { recursive: true });
  const runId = randomUUID();
  const probes = [
    {
      name: "success_pro",
      query: "Node.js 最新 LTS 发布记录",
      engine: "search_pro",
      domain: "nodejs.org",
      expected: "success",
    },
    {
      name: "success_std",
      query: "Node.js 最新 LTS 发布记录",
      engine: "search_std",
      domain: "nodejs.org",
      expected: "success",
    },
    {
      name: "empty",
      query: `tellyouwhy-empty-${runId}`,
      domain: "example.invalid",
      expected: "empty",
    },
    {
      name: "invalid_key",
      query: "Node.js 发布记录",
      expected: "invalid_key",
      invalidKey: true,
    },
  ];
  const report = {
    runId,
    startedAt: new Date().toISOString(),
    endpoint,
    provenance:
      "Live public queries; response fields are allowlisted and truncated. No credentials or raw error messages are stored.",
    billing:
      "Published prices: search_pro CNY 0.03/call; search_std CNY 0.01/call. Actual account charges and quota are not returned by this API and remain unverified.",
    attempts: [],
  };
  for (const probe of probes) {
    const request = buildRequest(probe);
    const started = performance.now();
    let attempt;
    try {
      const response = await fetch(endpoint, {
        method: "POST",
        redirect: "error",
        headers: {
          Authorization: `Bearer ${probe.invalidKey ? "p0-deliberately-invalid-token" : apiKey}`,
          "Content-Type": "application/json",
        },
        body: JSON.stringify(request),
        signal: AbortSignal.timeout(20_000),
      });
      const body = await readBoundedJson(response);
      const outcome = classifyResponse(response.status, body);
      attempt = {
        name: probe.name,
        request,
        httpStatus: response.status,
        outcome,
        matchesExpected: outcome === probe.expected,
        durationMs: Math.round(performance.now() - started),
        response: sanitizeResponse(body, apiKey),
      };
    } catch (error) {
      attempt = {
        name: probe.name,
        outcome:
          error?.name === "TimeoutError"
            ? "timeout"
            : "transport_or_parse_error",
        matchesExpected: false,
        durationMs: Math.round(performance.now() - started),
      };
    }
    report.attempts.push(attempt);
    console.log(`${probe.name}: ${attempt.outcome} (${attempt.durationMs} ms)`);
    // Stop paid probes on an account problem. There are no automatic retries.
    if (
      !probe.invalidKey &&
      ["invalid_key", "permission_denied", "insufficient_balance"].includes(
        attempt.outcome,
      )
    )
      break;
  }
  report.contractProbePassed =
    report.attempts.length === probes.length &&
    report.attempts.every((attempt) => attempt.matchesExpected);
  report.finishedAt = new Date().toISOString();
  const outputFile = new URL(`p0-live-${runId}.json`, outputDir);
  await writeFile(outputFile, `${JSON.stringify(report, null, 2)}\n`, "utf8");
  console.log(`脱敏报告：${fileURLToPath(outputFile)}`);
  console.log(
    `接口探测：${report.contractProbePassed ? "通过" : "未通过"}；内容、账号配额和实际扣费另行核验。`,
  );
  if (!report.contractProbePassed) process.exitCode = 1;
}
