import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, vi } from "vitest";
import {
  mcpListServers,
  mcpLogs,
  mcpDeleteCredential,
  mcpSaveServer,
  mcpSetItemEnabled,
  mcpTestConnection,
  type McpServer,
} from "../lib/mcp";
import { McpSettingsScreen } from "./McpSettingsScreen";

vi.mock("../lib/mcp", async (importOriginal) => {
  const actual = await importOriginal();
  if (!actual || typeof actual !== "object") {
    throw new Error("Failed to load the real MCP module for this test");
  }
  return {
    ...actual,
    mcpListServers: vi.fn(),
    mcpLogs: vi.fn(),
    mcpSaveServer: vi.fn(),
    mcpDeleteServer: vi.fn(),
    mcpDeleteCredential: vi.fn(),
    mcpSetServerEnabled: vi.fn(),
    mcpSetItemEnabled: vi.fn(),
    mcpTestConnection: vi.fn(),
  };
});

const baseServer: McpServer = {
  id: "local-notes",
  name: "Local Notes",
  transport: {
    transport: "stdio",
    command: "node",
    args: ["server.mjs"],
    cwd: "D:\\mcp",
  },
  enabled: true,
  credential_configured: false,
  key_last4: null,
  status: "disconnected",
  protocol_version: null,
  capabilities: {},
  instructions: null,
  last_error: null,
  last_connected_at: null,
  created_at: "2026-09-26T00:00:00Z",
  updated_at: "2026-09-26T00:00:00Z",
  items: [],
};

const discovered: McpServer = {
  ...baseServer,
  status: "connected",
  protocol_version: "2025-11-25",
  capabilities: { tools: {}, resources: {}, prompts: {} },
  items: [
    {
      kind: "tool",
      name: "search_notes",
      enabled: true,
      schema_hash: "abc",
      record: {
        name: "search_notes",
        description: "Search local notes",
        inputSchema: { type: "object" },
      },
      discovered_at: "2026-09-26T00:00:01Z",
    },
    {
      kind: "resource",
      name: "notes://guide",
      enabled: true,
      schema_hash: "def",
      record: { uri: "notes://guide", name: "Guide" },
      discovered_at: "2026-09-26T00:00:01Z",
    },
    {
      kind: "prompt",
      name: "study_notes",
      enabled: true,
      schema_hash: "ghi",
      record: { name: "study_notes" },
      discovered_at: "2026-09-26T00:00:01Z",
    },
  ],
};

const listMock = vi.mocked(mcpListServers);
const testMock = vi.mocked(mcpTestConnection);
const logsMock = vi.mocked(mcpLogs);
const toggleItemMock = vi.mocked(mcpSetItemEnabled);
const saveMock = vi.mocked(mcpSaveServer);
const deleteCredentialMock = vi.mocked(mcpDeleteCredential);

describe("McpSettingsScreen", () => {
  beforeEach(() => {
    listMock.mockReset().mockResolvedValue([baseServer]);
    testMock.mockReset().mockResolvedValue(discovered);
    logsMock.mockReset().mockResolvedValue(["server ready"]);
    toggleItemMock.mockReset().mockImplementation((_id, _kind, name, enabled) =>
      Promise.resolve({
        ...discovered,
        items: discovered.items.map((item) =>
          item.name === name ? { ...item, enabled } : item,
        ),
      }),
    );
    saveMock.mockReset();
    deleteCredentialMock.mockReset();
  });

  it("discovers Tools, Resources and Prompts, exposes logs, and disables one tool", async () => {
    const user = userEvent.setup();
    render(<McpSettingsScreen />);

    expect(
      await screen.findByRole("heading", { name: "Local Notes" }),
    ).toBeVisible();
    await user.click(screen.getByRole("button", { name: "测试并发现能力" }));

    expect(await screen.findByText("search_notes")).toBeVisible();
    expect(screen.getByText("notes://guide")).toBeVisible();
    expect(screen.getByText("study_notes")).toBeVisible();
    expect(screen.getByText("server ready")).toBeVisible();
    expect(testMock).toHaveBeenCalledWith("local-notes");

    await user.click(
      screen.getByRole("checkbox", { name: "启用工具 search_notes" }),
    );
    await waitFor(() =>
      expect(toggleItemMock).toHaveBeenCalledWith(
        "local-notes",
        "tool",
        "search_notes",
        false,
      ),
    );
    expect(
      screen.getByRole("checkbox", { name: "启用工具 search_notes" }),
    ).not.toBeChecked();
  });

  it("saves a Streamable HTTP server and passes its token only as a credential field", async () => {
    listMock.mockResolvedValue([]);
    const remote: McpServer = {
      ...baseServer,
      id: "remote-docs",
      name: "Remote Docs",
      transport: {
        transport: "streamable_http",
        url: "https://example.com/mcp",
      },
      credential_configured: true,
      key_last4: "oken",
    };
    saveMock.mockResolvedValue(remote);
    deleteCredentialMock.mockResolvedValue({
      ...remote,
      credential_configured: false,
      key_last4: null,
      status: "disconnected",
    });
    const user = userEvent.setup();
    render(<McpSettingsScreen />);

    await user.click(await screen.findByRole("button", { name: "添加服务器" }));
    await user.type(screen.getByLabelText("名称"), "Remote Docs");
    await user.selectOptions(
      screen.getByLabelText("连接方式"),
      "streamable_http",
    );
    await user.type(
      screen.getByLabelText("MCP URL"),
      "https://example.com/mcp",
    );
    await user.type(
      screen.getByLabelText("Bearer Token（可选）"),
      "secret-token",
    );
    await user.type(
      screen.getByLabelText("本地代理 URL（可选）"),
      "http://127.0.0.1:7897",
    );
    await user.click(screen.getByRole("button", { name: "保存配置" }));

    await waitFor(() => expect(saveMock).toHaveBeenCalledTimes(1));
    expect(saveMock).toHaveBeenCalledWith({
      id: null,
      name: "Remote Docs",
      transport: {
        transport: "streamable_http",
        url: "https://example.com/mcp",
        proxy_url: "http://127.0.0.1:7897",
      },
      enabled: true,
      bearer_token: "secret-token",
      replace_existing_credential: false,
    });
    expect(
      await screen.findByText("凭据未进入模型上下文。", { exact: false }),
    ).toBeVisible();

    await user.click(screen.getByRole("button", { name: "移除凭据" }));
    const dialog = await screen.findByRole("dialog", {
      name: "确认移除 Remote Docs 的凭据吗？",
    });
    await user.click(within(dialog).getByRole("button", { name: "移除凭据" }));
    await waitFor(() =>
      expect(deleteCredentialMock).toHaveBeenCalledWith("remote-docs"),
    );
    expect(
      await screen.findByText("请重新测试连接", { exact: false }),
    ).toBeVisible();
  });
});
