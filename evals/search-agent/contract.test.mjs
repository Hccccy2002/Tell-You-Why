import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import {
  buildRequest,
  classifyResponse,
  maxResponseBytes,
  readBoundedJson,
  sanitizeResponse,
} from "./contract.mjs";

const fixture = JSON.parse(
  readFileSync(new URL("./fixtures/contract.json", import.meta.url), "utf8"),
);

test("search is explicit and private identity is not added", () => {
  const request = buildRequest({ query: "Node.js 发布记录" });
  assert.equal(request.search_intent, false);
  assert.equal(request.search_engine, "search_pro");
  assert.equal(request.count, 5);
  assert.equal(request.content_size, "medium");
  assert.equal("user_id" in request, false);
  assert.equal("messages" in request, false);
});

test("query limits use Unicode characters and reject controls", () => {
  assert.doesNotThrow(() => buildRequest({ query: "知".repeat(70) }));
  assert.doesNotThrow(() => buildRequest({ query: "🌍".repeat(70) }));
  for (const query of ["", " ", "知".repeat(71), "query\nsecret", null])
    assert.throws(() => buildRequest({ query }), /invalid_query/u);
});

test("engine-specific counts and filters follow the documented capabilities", () => {
  for (const count of [0, 51, 1.5])
    assert.throws(
      () => buildRequest({ query: "test", count }),
      /invalid_count/u,
    );
  assert.throws(
    () => buildRequest({ query: "test", engine: "search_pro_sogou" }),
    /invalid_sogou_count/u,
  );
  assert.equal(
    buildRequest({ query: "test", engine: "search_pro_sogou", count: 10 })
      .count,
    10,
  );
  const quark = buildRequest({ query: "test", engine: "search_pro_quark" });
  assert.equal("count" in quark, false);
  assert.throws(() => buildRequest({ query: "test", engine: "unknown" }));
  assert.throws(() =>
    buildRequest({
      query: "test",
      engine: "search_pro_quark",
      domain: "nodejs.org",
    }),
  );
  assert.throws(() =>
    buildRequest({ query: "test", domain: "https://nodejs.org" }),
  );
  assert.throws(() => buildRequest({ query: "test", recency: "yesterday" }));
  assert.throws(() => buildRequest({ query: "test", requestId: "x" }));
});

test("success, empty and business errors remain distinct", () => {
  for (const sample of fixture.cases)
    assert.equal(
      classifyResponse(sample.httpStatus, sample.body),
      sample.expected,
    );
  assert.equal(
    classifyResponse(200, { search_result: [null] }),
    "malformed_response",
  );
  assert.equal(classifyResponse(200, {}), "malformed_response");
  assert.equal(classifyResponse(200, []), "malformed_response");
});

test("response provenance does not invent publication dates or keep secrets", () => {
  const secret = "p0-test-do-not-persist";
  const sanitized = sanitizeResponse(
    {
      request_id: "public-id",
      error: { code: 1000, message: secret },
      headers: { authorization: secret },
      search_result: [
        {
          title: secret,
          link: "https://nodejs.org/en/blog/",
          content: "公开摘要",
        },
      ],
    },
    secret,
  );
  assert.equal(JSON.stringify(sanitized).includes(secret), false);
  assert.equal(sanitized.results[0].publishedAt, null);
  assert.equal("headers" in sanitized, false);
  assert.equal("message" in sanitized, false);
});

test("response size is bounded while reading, and invalid JSON is rejected", async () => {
  assert.deepEqual(
    await readBoundedJson(new Response('{"search_result":[]}')),
    {
      search_result: [],
    },
  );
  await assert.rejects(readBoundedJson(new Response("x")), SyntaxError);
  await assert.rejects(
    readBoundedJson(new Response("x".repeat(maxResponseBytes + 1))),
    /response_too_large/u,
  );
});
