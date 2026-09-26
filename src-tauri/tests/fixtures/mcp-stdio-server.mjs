import readline from "node:readline";

const lines = readline.createInterface({ input: process.stdin });

lines.on("line", (line) => {
  const request = JSON.parse(line);
  if (request.id === undefined) return;

  let result;
  switch (request.method) {
    case "initialize":
      result = {
        protocolVersion: request.params.protocolVersion,
        capabilities: { tools: {}, resources: {}, prompts: {} },
        serverInfo: { name: "tell-you-why-test", version: "1.0.0" },
      };
      break;
    case "tools/list":
      result = {
        tools: [
          {
            name: "echo",
            description: "Echo a value",
            inputSchema: {
              type: "object",
              properties: { value: { type: "string" } },
              required: ["value"],
            },
          },
        ],
      };
      break;
    case "resources/list":
      result = { resources: [{ uri: "test://guide", name: "Test guide" }] };
      break;
    case "prompts/list":
      result = { prompts: [{ name: "hello", description: "Test prompt" }] };
      break;
    case "tools/call":
      result = {
        content: [{ type: "text", text: request.params.arguments.value }],
      };
      break;
    default:
      process.stdout.write(
        `${JSON.stringify({
          jsonrpc: "2.0",
          id: request.id,
          error: { code: -32601, message: "Method not found" },
        })}\n`,
      );
      return;
  }

  process.stdout.write(
    `${JSON.stringify({ jsonrpc: "2.0", id: request.id, result })}\n`,
  );
});
