// P0 contract probe only. The desktop adapter is implemented after the P0 gate.
import { randomUUID } from "node:crypto";

export const endpoint = "https://open.bigmodel.cn/api/paas/v4/web_search";
export const maxResponseBytes = 1_000_000;
export const engines = [
  "search_std",
  "search_pro",
  "search_pro_sogou",
  "search_pro_quark",
];
const recencies = ["oneDay", "oneWeek", "oneMonth", "oneYear", "noLimit"];

export function buildRequest({
  query,
  engine = "search_pro",
  count = 5,
  recency = "noLimit",
  domain,
  requestId = randomUUID(),
}) {
  if (
    typeof query !== "string" ||
    !query.trim() ||
    [...query.trim()].length > 70
  )
    throw new Error("invalid_query");
  if (/[\u0000-\u001f\u007f]/u.test(query)) throw new Error("invalid_query");
  if (!engines.includes(engine)) throw new Error("invalid_engine");
  if (!Number.isInteger(count) || count < 1 || count > 50)
    throw new Error("invalid_count");
  if (engine === "search_pro_sogou" && ![10, 20, 30, 40, 50].includes(count))
    throw new Error("invalid_sogou_count");
  if (!recencies.includes(recency)) throw new Error("invalid_recency");
  if (typeof requestId !== "string" || !/^[\w-]{6,64}$/u.test(requestId))
    throw new Error("invalid_request_id");
  if (
    domain &&
    !/^(?:[a-z0-9](?:[a-z0-9-]*[a-z0-9])?\.)+[a-z]{2,}$/iu.test(domain)
  )
    throw new Error("invalid_domain");
  if (engine === "search_pro_quark" && domain)
    throw new Error("unsupported_domain_filter");
  return {
    search_query: query.trim(),
    search_engine: engine,
    search_intent: false,
    ...(engine === "search_pro_quark" ? {} : { count }),
    search_recency_filter: recency,
    content_size: "medium",
    request_id: requestId,
    ...(domain ? { search_domain_filter: domain } : {}),
  };
}

export function classifyResponse(status, body) {
  if (!body || typeof body !== "object" || Array.isArray(body))
    return "malformed_response";
  const code = String(body.error?.code ?? "");
  if (code === "1113") return "insufficient_balance";
  if (["1000", "1001", "1002", "1003", "1004"].includes(code) || status === 401)
    return "invalid_key";
  if (status === 403 || ["1311", "1315"].includes(code))
    return "permission_denied";
  if (code === "1703") return "empty";
  if (code === "1702" || status >= 500) return "unavailable";
  if (status === 429 || ["1302", "1701"].includes(code)) return "rate_limited";
  if (body.error || status < 200 || status >= 300) return "request_rejected";
  if (!Array.isArray(body.search_result)) return "malformed_response";
  if (body.search_intent?.some((intent) => intent.intent === "SEARCH_NONE"))
    return "search_skipped";
  if (body.search_result.length === 0) return "empty";
  return body.search_result.every(
    (item) => item && typeof item === "object" && !Array.isArray(item),
  )
    ? "success"
    : "malformed_response";
}

export function sanitizeResponse(body, apiKey) {
  // Persist only public result fields. Never persist error messages or request headers.
  const clean = (value, limit = 1500) =>
    typeof value === "string"
      ? value
          .split(apiKey || "\u0000")
          .join("[REDACTED]")
          .replace(/[\u0000-\u0008\u000b\u000c\u000e-\u001f]/gu, "")
          .slice(0, limit)
      : null;
  return {
    requestId: clean(body?.request_id, 128),
    errorCode: /^[0-9]{3,6}$/u.test(String(body?.error?.code ?? ""))
      ? String(body.error.code)
      : null,
    intents: Array.isArray(body?.search_intent)
      ? body.search_intent.map((item) => clean(item?.intent, 32)).slice(0, 5)
      : [],
    results: Array.isArray(body?.search_result)
      ? body.search_result.slice(0, 10).map((item) => ({
          title: clean(item?.title, 300),
          url: clean(item?.link, 2000),
          snippet: clean(item?.content),
          publisher: clean(item?.media, 150),
          publishedAt: clean(item?.publish_date, 80),
          providerRefer: clean(item?.refer, 80),
        }))
      : [],
  };
}

export async function readBoundedJson(response) {
  const reader = response.body?.getReader();
  if (!reader) throw new Error("empty_body");
  let size = 0;
  const chunks = [];
  try {
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      size += value.byteLength;
      if (size > maxResponseBytes) throw new Error("response_too_large");
      chunks.push(Buffer.from(value));
    }
    return JSON.parse(Buffer.concat(chunks).toString("utf8"));
  } finally {
    await reader.cancel().catch(() => {});
    reader.releaseLock();
  }
}
