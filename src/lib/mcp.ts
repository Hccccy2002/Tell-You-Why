import { invoke } from "@tauri-apps/api/core";
import { isDesktop } from "./api";

export type McpTransport =
  | { transport: "stdio"; command: string; args: string[]; cwd: string }
  | {
      transport: "streamable_http";
      url: string;
      proxy_url?: string | null;
    };

export interface McpItem {
  kind: "tool" | "resource" | "prompt";
  name: string;
  enabled: boolean;
  schema_hash: string;
  record: Record<string, unknown>;
  discovered_at: string;
}

export interface McpServer {
  id: string;
  name: string;
  transport: McpTransport;
  enabled: boolean;
  credential_configured: boolean;
  key_last4: string | null;
  status: "disconnected" | "connected" | "error";
  protocol_version: string | null;
  capabilities: Record<string, unknown>;
  instructions: string | null;
  last_error: string | null;
  last_connected_at: string | null;
  created_at: string;
  updated_at: string;
  items: McpItem[];
}

export interface SaveMcpServerRequest {
  id?: string | null;
  name: string;
  transport: McpTransport;
  enabled: boolean;
  bearer_token?: string | null;
  replace_existing_credential?: boolean;
}

export const MCP_CREDENTIAL_REPLACEMENT_REQUIRED =
  "目标 MCP 服务器已有凭据，请确认覆盖后重试";

export const mcpListServers = async (): Promise<McpServer[]> =>
  isDesktop() ? invoke<McpServer[]>("mcp_list_servers") : [];

export const mcpSaveServer = (request: SaveMcpServerRequest) =>
  invoke<McpServer>("mcp_save_server", { request });

export const mcpDeleteServer = (id: string) =>
  invoke<void>("mcp_delete_server", { id });

export const mcpDeleteCredential = (id: string) =>
  invoke<McpServer>("mcp_delete_credential", { id });

export const mcpTestConnection = (id: string) =>
  invoke<McpServer>("mcp_test_connection", { id });

export const mcpSetServerEnabled = (id: string, enabled: boolean) =>
  invoke<McpServer>("mcp_set_server_enabled", { id, enabled });

export const mcpSetItemEnabled = (
  serverId: string,
  kind: McpItem["kind"],
  name: string,
  enabled: boolean,
) =>
  invoke<McpServer>("mcp_set_item_enabled", {
    serverId,
    kind,
    name,
    enabled,
  });

export const mcpLogs = (id: string) => invoke<string[]>("mcp_logs", { id });
