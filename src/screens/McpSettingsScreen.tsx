import { useEffect, useMemo, useState } from "react";
import { ConfirmationDialog } from "../components/ConfirmationDialog";
import { friendlyError } from "../lib/api";
import {
  MCP_CREDENTIAL_REPLACEMENT_REQUIRED,
  mcpDeleteCredential,
  mcpDeleteServer,
  mcpListServers,
  mcpLogs,
  mcpSaveServer,
  mcpSetItemEnabled,
  mcpSetServerEnabled,
  mcpTestConnection,
  type McpServer,
  type McpTransport,
  type SaveMcpServerRequest,
} from "../lib/mcp";

type TransportKind = McpTransport["transport"];

interface EditorState {
  id: string | null;
  name: string;
  transport: TransportKind;
  command: string;
  args: string;
  cwd: string;
  url: string;
  proxyUrl: string;
  bearerToken: string;
  enabled: boolean;
}

const emptyEditor = (): EditorState => ({
  id: null,
  name: "",
  transport: "stdio",
  command: "",
  args: "",
  cwd: "",
  url: "",
  proxyUrl: "",
  bearerToken: "",
  enabled: true,
});

function toEditor(server: McpServer): EditorState {
  return {
    ...emptyEditor(),
    id: server.id,
    name: server.name,
    transport: server.transport.transport,
    command:
      server.transport.transport === "stdio" ? server.transport.command : "",
    args:
      server.transport.transport === "stdio"
        ? server.transport.args.join("\n")
        : "",
    cwd: server.transport.transport === "stdio" ? server.transport.cwd : "",
    url:
      server.transport.transport === "streamable_http"
        ? server.transport.url
        : "",
    proxyUrl:
      server.transport.transport === "streamable_http"
        ? (server.transport.proxy_url ?? "")
        : "",
    bearerToken: "",
    enabled: server.enabled,
  };
}

function statusLabel(server: McpServer) {
  if (server.status === "connected") return "已连接";
  if (server.status === "error") return "连接错误";
  return "未连接";
}

export function McpSettingsScreen() {
  const [servers, setServers] = useState<McpServer[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [editor, setEditor] = useState<EditorState | null>(null);
  const [busy, setBusy] = useState<string | null>("load");
  const [message, setMessage] = useState<string | null>(null);
  const [logs, setLogs] = useState<string[]>([]);
  const [deleteTarget, setDeleteTarget] = useState<McpServer | null>(null);
  const [credentialTarget, setCredentialTarget] = useState<McpServer | null>(
    null,
  );
  const [replaceRequest, setReplaceRequest] =
    useState<SaveMcpServerRequest | null>(null);

  const selected = useMemo(
    () => servers.find((server) => server.id === selectedId) ?? null,
    [selectedId, servers],
  );

  useEffect(() => {
    let active = true;
    void mcpListServers()
      .then((items) => {
        if (!active) return;
        setServers(items);
        setSelectedId((current) => current ?? items[0]?.id ?? null);
      })
      .catch((error: unknown) => active && setMessage(friendlyError(error)))
      .finally(() => active && setBusy(null));
    return () => {
      active = false;
    };
  }, []);

  function updateServer(server: McpServer) {
    setServers((items) => {
      const exists = items.some((item) => item.id === server.id);
      return exists
        ? items.map((item) => (item.id === server.id ? server : item))
        : [...items, server];
    });
    setSelectedId(server.id);
  }

  function buildRequest(replaceExistingCredential = false) {
    if (!editor) return null;
    const transport: McpTransport =
      editor.transport === "stdio"
        ? {
            transport: "stdio",
            command: editor.command.trim(),
            args: editor.args
              .split("\n")
              .map((value) => value.trim())
              .filter(Boolean),
            cwd: editor.cwd.trim(),
          }
        : {
            transport: "streamable_http",
            url: editor.url.trim(),
            proxy_url: editor.proxyUrl.trim() || null,
          };
    return {
      id: editor.id,
      name: editor.name.trim(),
      transport,
      enabled: editor.enabled,
      bearer_token: editor.bearerToken.trim() || null,
      replace_existing_credential: replaceExistingCredential,
    } satisfies SaveMcpServerRequest;
  }

  async function submit(request: SaveMcpServerRequest) {
    setBusy("save");
    setMessage(null);
    try {
      const saved = await mcpSaveServer(request);
      updateServer(saved);
      setEditor(null);
      setReplaceRequest(null);
      setMessage("MCP 服务器配置已保存；凭据未进入模型上下文。");
    } catch (error) {
      const text = friendlyError(error);
      if (
        request.bearer_token &&
        !request.replace_existing_credential &&
        text === MCP_CREDENTIAL_REPLACEMENT_REQUIRED
      ) {
        setReplaceRequest({ ...request, replace_existing_credential: true });
      } else {
        setMessage(text);
      }
    } finally {
      setBusy(null);
    }
  }

  async function testConnection(server: McpServer) {
    setBusy(`test:${server.id}`);
    setMessage(null);
    try {
      const tested = await mcpTestConnection(server.id);
      updateServer(tested);
      setLogs(await mcpLogs(server.id));
      setMessage(
        `连接成功，发现 ${tested.items.filter((item) => item.kind === "tool").length} 个工具。`,
      );
    } catch (error) {
      setMessage(friendlyError(error));
      const latest = await mcpListServers().catch(() => null);
      if (latest) setServers(latest);
      setLogs(await mcpLogs(server.id).catch(() => []));
    } finally {
      setBusy(null);
    }
  }

  async function toggleServer(server: McpServer) {
    setBusy(`toggle:${server.id}`);
    try {
      updateServer(await mcpSetServerEnabled(server.id, !server.enabled));
    } catch (error) {
      setMessage(friendlyError(error));
    } finally {
      setBusy(null);
    }
  }

  async function toggleTool(server: McpServer, name: string, enabled: boolean) {
    setBusy(`tool:${server.id}:${name}`);
    try {
      updateServer(await mcpSetItemEnabled(server.id, "tool", name, enabled));
    } catch (error) {
      setMessage(friendlyError(error));
    } finally {
      setBusy(null);
    }
  }

  async function removeServer(server: McpServer) {
    setBusy("delete");
    try {
      await mcpDeleteServer(server.id);
      setServers((items) => items.filter((item) => item.id !== server.id));
      setSelectedId((current) => (current === server.id ? null : current));
      setDeleteTarget(null);
      setLogs([]);
      setMessage("MCP 服务器及其系统凭据已删除。");
    } catch (error) {
      setMessage(friendlyError(error));
    } finally {
      setBusy(null);
    }
  }

  async function removeCredential(server: McpServer) {
    setBusy("delete-credential");
    try {
      updateServer(await mcpDeleteCredential(server.id));
      setCredentialTarget(null);
      setMessage("已移除 MCP Bearer Token，请重新测试连接。");
    } catch (error) {
      setMessage(friendlyError(error));
    } finally {
      setBusy(null);
    }
  }

  return (
    <main className="page-view mcp-settings">
      <div className="page-heading mcp-heading">
        <div>
          <span className="eyebrow">Model Context Protocol</span>
          <h1>MCP 服务器</h1>
          <p>连接本地或远程能力，并控制哪些工具可供学习 Agent 使用。</p>
        </div>
        <button
          className="primary-button"
          disabled={busy != null}
          onClick={() => setEditor(emptyEditor())}
        >
          添加服务器
        </button>
      </div>

      {message ? (
        <p className="mcp-message" role="status">
          {message}
        </p>
      ) : null}
      {busy === "load" ? (
        <p className="list-status">正在读取 MCP 配置…</p>
      ) : null}

      {editor ? (
        <section
          className="settings-card mcp-editor"
          aria-label="MCP 服务器编辑器"
        >
          <h2>{editor.id ? "编辑服务器" : "添加服务器"}</h2>
          <label className="field-label">
            名称
            <input
              value={editor.name}
              disabled={busy != null}
              onChange={(event) =>
                setEditor({ ...editor, name: event.target.value })
              }
            />
          </label>
          <label className="field-label">
            连接方式
            <select
              value={editor.transport}
              disabled={busy != null}
              onChange={(event) =>
                setEditor({
                  ...editor,
                  transport: event.target.value as TransportKind,
                })
              }
            >
              <option value="stdio">本地 stdio</option>
              <option value="streamable_http">远程 Streamable HTTP</option>
            </select>
          </label>
          {editor.transport === "stdio" ? (
            <>
              <label className="field-label">
                可执行命令
                <input
                  aria-label="可执行命令"
                  placeholder="例如：npx 或 node"
                  value={editor.command}
                  disabled={busy != null}
                  onChange={(event) =>
                    setEditor({ ...editor, command: event.target.value })
                  }
                />
              </label>
              <label className="field-label">
                参数（每行一个）
                <textarea
                  aria-label="参数（每行一个）"
                  rows={3}
                  value={editor.args}
                  disabled={busy != null}
                  onChange={(event) =>
                    setEditor({ ...editor, args: event.target.value })
                  }
                />
              </label>
              <label className="field-label">
                工作目录（绝对路径）
                <input
                  value={editor.cwd}
                  disabled={busy != null}
                  onChange={(event) =>
                    setEditor({ ...editor, cwd: event.target.value })
                  }
                />
              </label>
            </>
          ) : (
            <>
              <label className="field-label">
                MCP URL
                <input
                  type="url"
                  placeholder="https://example.com/mcp"
                  value={editor.url}
                  disabled={busy != null}
                  onChange={(event) =>
                    setEditor({ ...editor, url: event.target.value })
                  }
                />
              </label>
              <label className="field-label">
                Bearer Token（可选）
                <input
                  type="password"
                  autoComplete="new-password"
                  value={editor.bearerToken}
                  disabled={busy != null}
                  onChange={(event) =>
                    setEditor({ ...editor, bearerToken: event.target.value })
                  }
                />
              </label>
              <label className="field-label">
                本地代理 URL（可选）
                <input
                  type="url"
                  placeholder="例如：http://127.0.0.1:7897"
                  value={editor.proxyUrl}
                  disabled={busy != null}
                  onChange={(event) =>
                    setEditor({ ...editor, proxyUrl: event.target.value })
                  }
                />
              </label>
            </>
          )}
          <p className="security-note">
            <span aria-hidden="true">◈</span>
            Token 仅保存在 Windows
            凭据库，并只在传输层附加；不会写入数据库、日志或模型消息。
          </p>
          <div className="mcp-form-actions">
            <button disabled={busy != null} onClick={() => setEditor(null)}>
              取消
            </button>
            <button
              className="primary-button"
              disabled={busy != null || !editor.name.trim()}
              onClick={() => {
                const request = buildRequest();
                if (request) void submit(request);
              }}
            >
              {busy === "save" ? "保存中…" : "保存配置"}
            </button>
          </div>
        </section>
      ) : null}

      {!editor && busy !== "load" && servers.length === 0 ? (
        <div className="empty-state">
          <span aria-hidden="true">⌁</span>
          <h2>还没有 MCP 服务器</h2>
          <p>可添加本地 stdio Server，或通过 HTTPS 连接远程 Server。</p>
        </div>
      ) : null}

      <div className="mcp-layout">
        <aside className="mcp-server-list" aria-label="MCP 服务器列表">
          {servers.map((server) => (
            <button
              key={server.id}
              className={
                server.id === selectedId ? "mcp-server active" : "mcp-server"
              }
              onClick={() => {
                setSelectedId(server.id);
                setLogs([]);
              }}
            >
              <span>{server.name}</span>
              <small className={`mcp-status ${server.status}`}>
                {statusLabel(server)}
              </small>
            </button>
          ))}
        </aside>

        {selected ? (
          <section className="mcp-detail" aria-label={`${selected.name} 详情`}>
            <div className="mcp-detail-heading">
              <div>
                <h2>{selected.name}</h2>
                <p>
                  {selected.transport.transport === "stdio"
                    ? `stdio · ${selected.transport.command}`
                    : selected.transport.url}
                </p>
              </div>
              <label className="mcp-switch">
                <input
                  type="checkbox"
                  checked={selected.enabled}
                  disabled={busy != null}
                  onChange={() => void toggleServer(selected)}
                />
                允许使用
              </label>
            </div>
            <div className="mcp-actions">
              <button
                disabled={busy != null}
                onClick={() => setEditor(toEditor(selected))}
              >
                编辑
              </button>
              <button
                disabled={busy != null}
                onClick={() => void testConnection(selected)}
              >
                {busy === `test:${selected.id}` ? "连接中…" : "测试并发现能力"}
              </button>
              <button
                disabled={busy != null}
                onClick={() => setDeleteTarget(selected)}
              >
                删除
              </button>
              {selected.credential_configured ? (
                <button
                  disabled={busy != null}
                  onClick={() => setCredentialTarget(selected)}
                >
                  移除凭据
                </button>
              ) : null}
            </div>
            <dl className="mcp-meta">
              <div>
                <dt>状态</dt>
                <dd>{statusLabel(selected)}</dd>
              </div>
              <div>
                <dt>协议</dt>
                <dd>{selected.protocol_version ?? "尚未协商"}</dd>
              </div>
              <div>
                <dt>凭据</dt>
                <dd>
                  {selected.credential_configured
                    ? `已配置 ····${selected.key_last4}`
                    : "未配置"}
                </dd>
              </div>
            </dl>
            {selected.last_error ? (
              <p className="mcp-error" role="alert">
                {selected.last_error}
              </p>
            ) : null}

            {(["tool", "resource", "prompt"] as const).map((kind) => {
              const items = selected.items.filter((item) => item.kind === kind);
              const label =
                kind === "tool"
                  ? "Tools"
                  : kind === "resource"
                    ? "Resources"
                    : "Prompts";
              return (
                <section className="mcp-capability" key={kind}>
                  <h3>
                    {label}
                    <span>{items.length}</span>
                  </h3>
                  {items.length === 0 ? (
                    <p>尚未发现</p>
                  ) : (
                    items.map((item) => (
                      <details key={`${kind}:${item.name}`}>
                        <summary>
                          <code>{item.name}</code>
                          {kind === "tool" ? (
                            <label
                              className="mcp-tool-toggle"
                              onClick={(event) => event.stopPropagation()}
                            >
                              <input
                                type="checkbox"
                                aria-label={`启用工具 ${item.name}`}
                                checked={item.enabled}
                                disabled={busy != null || !selected.enabled}
                                onChange={(event) =>
                                  void toggleTool(
                                    selected,
                                    item.name,
                                    event.target.checked,
                                  )
                                }
                              />
                              启用
                            </label>
                          ) : null}
                        </summary>
                        <pre>{JSON.stringify(item.record, null, 2)}</pre>
                      </details>
                    ))
                  )}
                </section>
              );
            })}

            <section className="mcp-capability">
              <div className="mcp-log-heading">
                <h3>启动日志</h3>
                <button
                  disabled={busy != null}
                  onClick={() =>
                    void mcpLogs(selected.id)
                      .then(setLogs)
                      .catch((error) => setMessage(friendlyError(error)))
                  }
                >
                  刷新
                </button>
              </div>
              <pre className="mcp-logs">
                {logs.length ? logs.join("\n") : "暂无日志"}
              </pre>
            </section>
          </section>
        ) : null}
      </div>

      {deleteTarget ? (
        <ConfirmationDialog
          id="delete-mcp-server"
          title={`确认删除 ${deleteTarget.name} 吗？`}
          confirmLabel="删除服务器"
          busyLabel="正在删除…"
          busy={busy === "delete"}
          onCancel={() => setDeleteTarget(null)}
          onConfirm={() => void removeServer(deleteTarget)}
        >
          <p>服务器配置、能力快照和系统凭据都会一并删除。</p>
        </ConfirmationDialog>
      ) : null}
      {credentialTarget ? (
        <ConfirmationDialog
          id="delete-mcp-credential"
          title={`确认移除 ${credentialTarget.name} 的凭据吗？`}
          confirmLabel="移除凭据"
          busyLabel="正在移除…"
          busy={busy === "delete-credential"}
          onCancel={() => setCredentialTarget(null)}
          onConfirm={() => void removeCredential(credentialTarget)}
        >
          <p>只删除 Windows 凭据库中的 Bearer Token，保留服务器配置。</p>
        </ConfirmationDialog>
      ) : null}
      {replaceRequest ? (
        <ConfirmationDialog
          id="replace-mcp-token"
          title="确认覆盖 MCP 凭据吗？"
          confirmLabel="确认覆盖凭据"
          busyLabel="正在保存…"
          busy={busy === "save"}
          onCancel={() => setReplaceRequest(null)}
          onConfirm={() => void submit(replaceRequest)}
        >
          <p>原凭据无法查看；确认后会在 Windows 凭据库中被新 Token 替换。</p>
        </ConfirmationDialog>
      ) : null}
    </main>
  );
}
