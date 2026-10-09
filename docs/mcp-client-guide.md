# MCP Client 使用与安全边界

入口：**菜单 → 开发者工具 → MCP 服务器**。Tell You Why 作为 MCP Client，支持本地 `stdio` 与远程 Streamable HTTP Server，并将明确允许的外部 Tools 接入 PDF 复习 Agent。

## 添加服务器

### 本地 stdio

选择“本地 stdio”，分别填写可执行命令、参数和绝对工作目录。参数每行一个，应用直接启动进程，不通过 shell 拼接命令。连接测试会启动进程，完成 MCP 初始化，并读取 Server 暴露的 Tools、Resources 和 Prompts。

适合验证 Client 的官方参考 Server 是 `@modelcontextprotocol/server-everything`。安装 Node.js 后，可填写：

- 命令：`npx`
- 参数：`-y`、`@modelcontextprotocol/server-everything`、`stdio`（每项一行）
- 工作目录：任意已存在且可信的绝对目录

只添加你信任的本地 Server；它以当前 Windows 用户权限运行，能力不受模型工具权限本身限制。

### 远程 Streamable HTTP

选择“远程 Streamable HTTP”并填写 MCP URL。除 `localhost`、`127.0.0.1` 和 `::1` 外必须使用 HTTPS，不跟随重定向。可选 Bearer Token 保存到 Windows 凭据库；SQLite 仅记录凭据引用与尾号。

公开或匿名 Server 不要填写 Token。如果配置曾保存过 Token，可在服务器详情点击“移除凭据”，然后重新测试连接；仅把编辑框留空会保留已有凭据。

若系统所在网络需要本地代理，可在“本地代理 URL”填写回环地址，例如 `http://127.0.0.1:7897`。为避免把 MCP 流量意外发送给第三方，界面配置只接受 `localhost` 或回环 IP 的 HTTP/HTTPS 代理，不支持在 URL 中写入代理凭据。

无需凭据的公开测试地址：`https://developers.openai.com/mcp`。它适合验证初始化和 Tools 发现。生产 Server 应使用自己的授权、限流和审计策略。

## 能力与权限

连接测试成功后，页面显示：

- Tools：名称、描述、Input Schema，并可逐项启用或停用；
- Resources：URI 与 Server 返回的元数据；
- Prompts：名称、参数和描述；
- 协议版本、连接状态、启动日志和 Schema/连接错误。

当前 Agent 只自动调用 Tools。Resources 与 Prompts 可发现和查看，Rust 命令层也支持读取/获取，但尚未在复习界面自动注入。

进入 **PDF → 打开教材 → 复习 Agent** 后，在“本次允许的 MCP 服务器”中逐项勾选。未勾选的 Server 不会进入模型工具列表。会话开始时会固定服务器、工具名、Input Schema 和 Schema 哈希；后续新增工具不会静默扩大旧会话权限。若 Server 被删除、停用，或 Schema 发生变化，旧会话中的调用会失败并要求新建会话。

## 数据边界

- Bearer Token 只在 Rust 传输层从系统凭据库读取并加入 HTTP 请求，不进入模型消息、工具定义、SQLite 或 Trace。
- MCP Tool 的名称、描述和 Input Schema 会发送给当前生成模型，因为模型需要据此选择工具。
- 模型生成的工具参数会发送给被选中的 MCP Server；Server 返回的 Tool 结果会回传给模型并可能影响回答。
- Trace 只保存 Server/Tool 标识、Schema 哈希、参数键名、状态与结果大小，不保存 Token、完整参数或完整结果。
- MCP 返回值属于不可信外部数据，不能改变系统规则；仍应避免把敏感资料发送给不可信 Server。

## 测试

仓库包含两类测试：

1. `src-tauri/tests/fixtures/mcp-stdio-server.mjs`：无需联网，验证真实子进程初始化、发现、调用以及 Agent 会话快照执行。
2. `openai_docs_streamable_http_smoke`：默认忽略的联网测试，验证公开 OpenAI Docs Streamable HTTP Server。

运行命令：

```powershell
cargo test --manifest-path src-tauri/Cargo.toml mcp::tests --offline
cargo test --manifest-path src-tauri/Cargo.toml openai_docs_streamable_http_smoke -- --ignored
```

## 当前边界

- 远程凭据首版支持静态 Bearer Token，尚未实现 OAuth 2.1 浏览器授权与 Token 刷新。
- 每次复习最多选择 5 个 Server、24 个外部 Tools；单次工具参数、结果、连接时间及总 Agent 调用均有上限。
- 外部 Tools 当前接入 PDF 复习 Agent；陪伴学习、知识卡生成和教材问答尚未开放 MCP 自动调用。
