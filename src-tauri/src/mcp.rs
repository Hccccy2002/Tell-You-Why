//! MCP client runtime, persistence, and Tauri commands.
//!
//! Secrets are resolved only at the transport boundary. Database records,
//! traces, logs, and model-visible tool definitions never contain credentials.

use crate::{
    db::{Database, DbError},
    harness::policy::StopReason,
    review_agent::{ReviewRun, ToolCall},
    secret_store::{last_four, SecretStore},
    AppState,
};
use async_trait::async_trait;
use reqwest::{header, redirect::Policy, Client, Proxy, Url};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, VecDeque},
    path::Path,
    process::Stdio,
    sync::Arc,
    time::{Duration, Instant},
};
use tauri::State;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout, Command},
    sync::Mutex,
};

const PROTOCOL_VERSION: &str = "2025-11-25";
const MAX_MESSAGE_BYTES: usize = 1_048_576;
const MAX_LOG_LINES: usize = 200;
const MCP_CREDENTIAL_REPLACEMENT_REQUIRED: &str = "目标 MCP 服务器已有凭据，请确认覆盖后重试";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "transport", rename_all = "snake_case", deny_unknown_fields)]
pub enum McpTransportConfig {
    Stdio {
        command: String,
        #[serde(default)]
        args: Vec<String>,
        cwd: String,
    },
    StreamableHttp {
        url: String,
        #[serde(default)]
        proxy_url: Option<String>,
    },
}

impl McpTransportConfig {
    fn kind(&self) -> &'static str {
        match self {
            Self::Stdio { .. } => "stdio",
            Self::StreamableHttp { .. } => "streamable_http",
        }
    }

    fn validate(&self) -> Result<(), String> {
        match self {
            Self::Stdio { command, args, cwd } => {
                if command.trim().is_empty() || command.chars().count() > 1024 {
                    return Err("stdio 命令不能为空且不能超过 1024 字符".into());
                }
                if args.len() > 32
                    || args
                        .iter()
                        .any(|arg| arg.chars().count() > 2048 || arg.contains('\0'))
                {
                    return Err("stdio 参数数量或长度超出限制".into());
                }
                let folder = Path::new(cwd);
                if !folder.is_absolute() || !folder.is_dir() {
                    return Err("stdio 工作目录必须是已存在的绝对路径".into());
                }
            }
            Self::StreamableHttp { url, proxy_url } => {
                validate_http_url(url)?;
                if let Some(proxy_url) = proxy_url.as_deref().filter(|value| !value.is_empty()) {
                    validate_proxy_url(proxy_url)?;
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct McpItem {
    pub kind: String,
    pub name: String,
    pub enabled: bool,
    pub schema_hash: String,
    pub record: Value,
    pub discovered_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct McpServer {
    pub id: String,
    pub name: String,
    pub transport: McpTransportConfig,
    pub enabled: bool,
    pub credential_ref: Option<String>,
    pub key_last4: Option<String>,
    pub status: String,
    pub protocol_version: Option<String>,
    pub capabilities: Value,
    pub instructions: Option<String>,
    pub last_error: Option<String>,
    pub last_connected_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub items: Vec<McpItem>,
}

impl McpServer {
    fn public(&self) -> Value {
        json!({
            "id": self.id,
            "name": self.name,
            "transport": self.transport,
            "enabled": self.enabled,
            "credential_configured": self.credential_ref.is_some() && self.key_last4.is_some(),
            "key_last4": self.key_last4,
            "status": self.status,
            "protocol_version": self.protocol_version,
            "capabilities": self.capabilities,
            "instructions": self.instructions,
            "last_error": self.last_error,
            "last_connected_at": self.last_connected_at,
            "created_at": self.created_at,
            "updated_at": self.updated_at,
            "items": self.items,
        })
    }
}

#[derive(Clone, Debug)]
struct Discovery {
    protocol_version: String,
    capabilities: Value,
    instructions: Option<String>,
    items: Vec<(String, String, Value)>,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum McpError {
    #[error("MCP 配置无效：{0}")]
    Configuration(String),
    #[error("无法启动 MCP 服务器：{0}")]
    Start(String),
    #[error("MCP 连接超时")]
    Timeout,
    #[error("MCP 服务器返回无效协议消息")]
    InvalidMessage,
    #[error("MCP 服务器返回错误：{0}")]
    Remote(String),
    #[error("MCP Schema 错误：{0}")]
    Schema(String),
    #[error("MCP 响应超过大小限制")]
    OutputLimit,
    #[error("MCP 连接已关闭")]
    Closed,
}

#[async_trait]
trait RpcPeer: Send {
    async fn request(&mut self, method: &str, params: Value) -> Result<Value, McpError>;
    async fn notify(&mut self, method: &str, params: Value) -> Result<(), McpError>;
    async fn logs(&self) -> Vec<String>;
}

struct StdioPeer {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
    logs: Arc<Mutex<VecDeque<String>>>,
}

impl StdioPeer {
    async fn connect(config: &McpTransportConfig) -> Result<Self, McpError> {
        let McpTransportConfig::Stdio { command, args, cwd } = config else {
            return Err(McpError::Configuration("不是 stdio 配置".into()));
        };
        let mut process = Command::new(command);
        process
            .args(args)
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        #[cfg(windows)]
        process.creation_flags(0x08000000);
        let mut child = process
            .spawn()
            .map_err(|error| McpError::Start(error.to_string()))?;
        let stdin = child.stdin.take().ok_or(McpError::Closed)?;
        let stdout = child.stdout.take().ok_or(McpError::Closed)?;
        let stderr = child.stderr.take().ok_or(McpError::Closed)?;
        let logs = Arc::new(Mutex::new(VecDeque::new()));
        let captured = logs.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let mut entries = captured.lock().await;
                if entries.len() == MAX_LOG_LINES {
                    entries.pop_front();
                }
                entries.push_back(redact_log(&line));
            }
        });
        Ok(Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
            next_id: 1,
            logs,
        })
    }

    async fn write_message(&mut self, value: &Value) -> Result<(), McpError> {
        let encoded = serde_json::to_vec(value).map_err(|_| McpError::InvalidMessage)?;
        if encoded.len() > MAX_MESSAGE_BYTES {
            return Err(McpError::OutputLimit);
        }
        self.stdin
            .write_all(&encoded)
            .await
            .map_err(|_| McpError::Closed)?;
        self.stdin
            .write_all(b"\n")
            .await
            .map_err(|_| McpError::Closed)?;
        self.stdin.flush().await.map_err(|_| McpError::Closed)
    }

    async fn read_response(&mut self, id: u64) -> Result<Value, McpError> {
        loop {
            let mut line = String::new();
            let read =
                tokio::time::timeout(Duration::from_secs(30), self.stdout.read_line(&mut line))
                    .await
                    .map_err(|_| McpError::Timeout)?
                    .map_err(|_| McpError::Closed)?;
            if read == 0 {
                return Err(McpError::Closed);
            }
            if line.len() > MAX_MESSAGE_BYTES {
                return Err(McpError::OutputLimit);
            }
            let value: Value =
                serde_json::from_str(line.trim()).map_err(|_| McpError::InvalidMessage)?;
            if value.get("id").and_then(Value::as_u64) != Some(id) {
                // Notifications and unrelated responses are valid on a shared channel.
                continue;
            }
            return rpc_result(value);
        }
    }
}

impl Drop for StdioPeer {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
    }
}

#[async_trait]
impl RpcPeer for StdioPeer {
    async fn request(&mut self, method: &str, params: Value) -> Result<Value, McpError> {
        let id = self.next_id;
        self.next_id += 1;
        self.write_message(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        }))
        .await?;
        self.read_response(id).await
    }

    async fn notify(&mut self, method: &str, params: Value) -> Result<(), McpError> {
        self.write_message(&json!({"jsonrpc":"2.0","method":method,"params":params}))
            .await
    }

    async fn logs(&self) -> Vec<String> {
        self.logs.lock().await.iter().cloned().collect()
    }
}

struct HttpPeer {
    client: Client,
    endpoint: Url,
    bearer: Option<String>,
    session_id: Option<String>,
    next_id: u64,
    logs: Arc<Mutex<VecDeque<String>>>,
}

impl HttpPeer {
    fn connect(config: &McpTransportConfig, bearer: Option<String>) -> Result<Self, McpError> {
        let McpTransportConfig::StreamableHttp { url, proxy_url } = config else {
            return Err(McpError::Configuration("不是 HTTP 配置".into()));
        };
        validate_http_url(url).map_err(McpError::Configuration)?;
        let endpoint =
            Url::parse(url).map_err(|error| McpError::Configuration(error.to_string()))?;
        let mut builder = Client::builder()
            .redirect(Policy::none())
            .connect_timeout(Duration::from_secs(8))
            .user_agent("TellYouWhy-MCP/0.2");
        if let Some(proxy_url) = proxy_url.as_deref().filter(|value| !value.is_empty()) {
            builder = builder.proxy(
                Proxy::all(proxy_url)
                    .map_err(|error| McpError::Configuration(error.to_string()))?,
            );
        }
        let client = builder
            .build()
            .map_err(|error| McpError::Start(error.to_string()))?;
        Ok(Self {
            client,
            endpoint,
            bearer,
            session_id: None,
            next_id: 1,
            logs: Arc::new(Mutex::new(VecDeque::new())),
        })
    }

    async fn send(&mut self, body: Value, expects_response: bool) -> Result<Value, McpError> {
        let mut request = self
            .client
            .post(self.endpoint.clone())
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::ACCEPT, "application/json, text/event-stream")
            .timeout(Duration::from_secs(30))
            .json(&body);
        if let Some(token) = &self.bearer {
            request = request.bearer_auth(token);
        }
        if let Some(session) = &self.session_id {
            request = request.header("Mcp-Session-Id", session);
        }
        let response = request.send().await.map_err(|error| {
            if error.is_timeout() {
                McpError::Timeout
            } else {
                McpError::Remote("网络连接失败".into())
            }
        })?;
        if self.session_id.is_none() {
            self.session_id = response
                .headers()
                .get("Mcp-Session-Id")
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned);
        }
        let status = response.status();
        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("")
            .to_owned();
        let bytes = response
            .bytes()
            .await
            .map_err(|_| McpError::InvalidMessage)?;
        if bytes.len() > MAX_MESSAGE_BYTES {
            return Err(McpError::OutputLimit);
        }
        if !status.is_success() {
            let message = if status.as_u16() == 403 {
                if self.bearer.is_some() {
                    "HTTP 403（服务器拒绝当前 Bearer Token；公开 Server 请移除已保存凭据）".into()
                } else {
                    "HTTP 403（服务器拒绝当前网络来源，请检查代理、出口 IP 或地区策略）".into()
                }
            } else {
                format!("HTTP {}", status.as_u16())
            };
            return Err(McpError::Remote(message));
        }
        if !expects_response {
            return Ok(Value::Null);
        }
        let text = std::str::from_utf8(&bytes).map_err(|_| McpError::InvalidMessage)?;
        let value = if content_type.contains("text/event-stream") {
            parse_sse_response(text)?
        } else {
            serde_json::from_str(text).map_err(|_| McpError::InvalidMessage)?
        };
        rpc_result(value)
    }
}

#[async_trait]
impl RpcPeer for HttpPeer {
    async fn request(&mut self, method: &str, params: Value) -> Result<Value, McpError> {
        let id = self.next_id;
        self.next_id += 1;
        self.send(
            json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}),
            true,
        )
        .await
    }

    async fn notify(&mut self, method: &str, params: Value) -> Result<(), McpError> {
        self.send(
            json!({"jsonrpc":"2.0","method":method,"params":params}),
            false,
        )
        .await?;
        Ok(())
    }

    async fn logs(&self) -> Vec<String> {
        self.logs.lock().await.iter().cloned().collect()
    }
}

pub struct McpRuntime {
    sessions: Mutex<HashMap<String, Box<dyn RpcPeer>>>,
}

impl McpRuntime {
    pub fn new() -> Result<Self, String> {
        Ok(Self {
            sessions: Mutex::new(HashMap::new()),
        })
    }

    async fn peer(
        config: &McpTransportConfig,
        bearer: Option<String>,
    ) -> Result<Box<dyn RpcPeer>, McpError> {
        match config {
            McpTransportConfig::Stdio { .. } => Ok(Box::new(StdioPeer::connect(config).await?)),
            McpTransportConfig::StreamableHttp { .. } => {
                Ok(Box::new(HttpPeer::connect(config, bearer)?))
            }
        }
    }

    async fn discover(
        &self,
        server: &McpServer,
        bearer: Option<String>,
    ) -> Result<Discovery, McpError> {
        let mut peer = Self::peer(&server.transport, bearer).await?;
        let result = initialize_and_discover(peer.as_mut()).await?;
        self.sessions.lock().await.insert(server.id.clone(), peer);
        Ok(result)
    }

    pub async fn disconnect(&self, id: &str) {
        self.sessions.lock().await.remove(id);
    }

    pub(crate) async fn request(
        &self,
        server: &McpServer,
        bearer: Option<String>,
        method: &str,
        params: Value,
    ) -> Result<Value, McpError> {
        let mut sessions = self.sessions.lock().await;
        if !sessions.contains_key(&server.id) {
            let mut peer = Self::peer(&server.transport, bearer).await?;
            initialize(peer.as_mut()).await?;
            sessions.insert(server.id.clone(), peer);
        }
        let outcome = sessions
            .get_mut(&server.id)
            .ok_or(McpError::Closed)?
            .request(method, params)
            .await;
        if outcome.is_err() {
            sessions.remove(&server.id);
        }
        outcome
    }

    async fn logs(&self, id: &str) -> Vec<String> {
        let sessions = self.sessions.lock().await;
        match sessions.get(id) {
            Some(peer) => peer.logs().await,
            None => vec![],
        }
    }
}

async fn initialize(peer: &mut dyn RpcPeer) -> Result<Value, McpError> {
    let initialized = peer
        .request(
            "initialize",
            json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": {"name":"Tell You Why","version":env!("CARGO_PKG_VERSION")},
            }),
        )
        .await?;
    peer.notify("notifications/initialized", json!({})).await?;
    Ok(initialized)
}

async fn initialize_and_discover(peer: &mut dyn RpcPeer) -> Result<Discovery, McpError> {
    let initialized = initialize(peer).await?;
    let protocol_version = initialized["protocolVersion"]
        .as_str()
        .unwrap_or(PROTOCOL_VERSION)
        .to_owned();
    let capabilities = initialized["capabilities"].clone();
    let instructions = initialized["instructions"].as_str().map(str::to_owned);
    let mut items = vec![];
    if capabilities.get("tools").is_some() {
        for item in list_all(peer, "tools/list", "tools").await? {
            let name = item["name"]
                .as_str()
                .filter(|name| !name.trim().is_empty() && name.chars().count() <= 256)
                .ok_or_else(|| McpError::Schema("Tool 缺少有效 name".into()))?;
            let schema = item
                .get("inputSchema")
                .ok_or_else(|| McpError::Schema(format!("Tool {name} 缺少 inputSchema")))?;
            if !schema.is_object() || schema["type"] != "object" {
                return Err(McpError::Schema(format!(
                    "Tool {name} 的 inputSchema 必须是 object"
                )));
            }
            items.push(("tool".into(), name.into(), item));
        }
    }
    if capabilities.get("resources").is_some() {
        for item in list_all(peer, "resources/list", "resources").await? {
            if let Some(uri) = item["uri"].as_str() {
                items.push(("resource".into(), uri.into(), item));
            }
        }
    }
    if capabilities.get("prompts").is_some() {
        for item in list_all(peer, "prompts/list", "prompts").await? {
            if let Some(name) = item["name"].as_str() {
                items.push(("prompt".into(), name.into(), item));
            }
        }
    }
    Ok(Discovery {
        protocol_version,
        capabilities,
        instructions,
        items,
    })
}

async fn list_all(
    peer: &mut dyn RpcPeer,
    method: &str,
    field: &str,
) -> Result<Vec<Value>, McpError> {
    let mut cursor: Option<String> = None;
    let mut output = vec![];
    for _ in 0..10 {
        let params = cursor
            .as_ref()
            .map_or_else(|| json!({}), |value| json!({"cursor":value}));
        let page = peer.request(method, params).await?;
        let values = page[field].as_array().ok_or(McpError::InvalidMessage)?;
        if output.len() + values.len() > 500 {
            return Err(McpError::OutputLimit);
        }
        output.extend(values.iter().cloned());
        cursor = page["nextCursor"].as_str().map(str::to_owned);
        if cursor.is_none() {
            return Ok(output);
        }
    }
    Err(McpError::OutputLimit)
}

fn rpc_result(value: Value) -> Result<Value, McpError> {
    if value["jsonrpc"] != "2.0" {
        return Err(McpError::InvalidMessage);
    }
    if !value["error"].is_null() {
        let message = value["error"]["message"]
            .as_str()
            .unwrap_or("未知 MCP 错误");
        return Err(McpError::Remote(message.chars().take(500).collect()));
    }
    value.get("result").cloned().ok_or(McpError::InvalidMessage)
}

fn parse_sse_response(text: &str) -> Result<Value, McpError> {
    let data = text
        .lines()
        .filter_map(|line| line.strip_prefix("data:"))
        .map(str::trim)
        .filter(|line| !line.is_empty() && *line != "[DONE]")
        .last()
        .ok_or(McpError::InvalidMessage)?;
    serde_json::from_str(data).map_err(|_| McpError::InvalidMessage)
}

fn validate_http_url(input: &str) -> Result<(), String> {
    let url = Url::parse(input).map_err(|_| "请输入完整 MCP URL")?;
    let host = url.host_str().ok_or("MCP URL 缺少主机名")?;
    let local = matches!(host, "localhost" | "127.0.0.1" | "::1");
    if (url.scheme() != "https" && !(local && url.scheme() == "http"))
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err("远程 MCP 必须使用 HTTPS；仅本机地址允许 HTTP".into());
    }
    Ok(())
}

fn validate_proxy_url(input: &str) -> Result<(), String> {
    let url = Url::parse(input).map_err(|_| "请输入完整的本地代理 URL")?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("MCP 代理只支持 http:// 或 https://".into());
    }
    let host = url.host_str().ok_or("MCP 代理 URL 缺少主机名")?;
    let local = host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|address| address.is_loopback());
    if !local {
        return Err("MCP 代理必须使用 localhost 或回环 IP".into());
    }
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err("MCP 代理 URL 不能包含凭据、查询参数或片段".into());
    }
    Ok(())
}

fn redact_log(input: &str) -> String {
    let mut value: String = input.chars().take(2000).collect();
    for marker in ["Authorization", "Bearer ", "api_key", "access_token"] {
        if let Some(index) = value
            .to_ascii_lowercase()
            .find(&marker.to_ascii_lowercase())
        {
            value.replace_range(index.., "[REDACTED]");
        }
    }
    value
}

fn schema_hash(value: &Value) -> String {
    let digest = Sha256::digest(value.to_string().as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn tool_alias(server_id: &str, remote_name: &str) -> String {
    let clean = |input: &str| {
        input
            .chars()
            .map(|character| {
                if character.is_ascii_alphanumeric() || character == '_' {
                    character.to_ascii_lowercase()
                } else {
                    '_'
                }
            })
            .collect::<String>()
    };
    let mut alias = format!("mcp__{}__{}", clean(server_id), clean(remote_name));
    if alias.len() > 64 {
        let suffix = &schema_hash(&json!([server_id, remote_name]))[..10];
        alias.truncate(52);
        alias.push('_');
        alias.push_str(suffix);
    }
    alias
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct McpToolSnapshot {
    pub name: String,
    pub alias: String,
    pub schema_hash: String,
    pub description: String,
    pub input_schema: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct McpServerSnapshot {
    pub id: String,
    pub name: String,
    pub protocol_version: String,
    pub tools: Vec<McpToolSnapshot>,
}

pub fn tool_definitions(snapshots: &[McpServerSnapshot]) -> Value {
    json!(snapshots
        .iter()
        .flat_map(|server| {
            server.tools.iter().map(move |tool| {
                json!({
                    "type":"function",
                    "function":{
                        "name":tool.alias,
                        "description":format!("来自 MCP 服务器 {}：{}", server.name, tool.description),
                        "parameters":tool.input_schema,
                    }
                })
            })
        })
        .collect::<Vec<_>>())
}

pub fn snapshot_tool<'a>(
    snapshots: &'a [McpServerSnapshot],
    alias: &str,
) -> Option<(&'a McpServerSnapshot, &'a McpToolSnapshot)> {
    snapshots.iter().find_map(|server| {
        server
            .tools
            .iter()
            .find(|tool| tool.alias == alias)
            .map(|tool| (server, tool))
    })
}

impl Database {
    pub(crate) fn mcp_list(&self) -> Result<Vec<McpServer>, DbError> {
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT id,name,transport,config_json,enabled,credential_ref,key_last4,status,
                    protocol_version,capabilities_json,instructions,last_error,last_connected_at,
                    created_at,updated_at FROM mcp_servers ORDER BY created_at,id",
        )?;
        let rows = statement
            .query_map([], |row| {
                let config: String = row.get(3)?;
                let capabilities: String = row.get(9)?;
                Ok(McpServer {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    transport: serde_json::from_str(&config).map_err(|error| {
                        rusqlite::Error::FromSqlConversionFailure(
                            3,
                            rusqlite::types::Type::Text,
                            Box::new(error),
                        )
                    })?,
                    enabled: row.get(4)?,
                    credential_ref: row.get(5)?,
                    key_last4: row.get(6)?,
                    status: row.get(7)?,
                    protocol_version: row.get(8)?,
                    capabilities: serde_json::from_str(&capabilities).unwrap_or_else(|_| json!({})),
                    instructions: row.get(10)?,
                    last_error: row.get(11)?,
                    last_connected_at: row.get(12)?,
                    created_at: row.get(13)?,
                    updated_at: row.get(14)?,
                    items: vec![],
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut result = Vec::with_capacity(rows.len());
        for mut server in rows {
            let mut items = connection.prepare(
                "SELECT kind,name,enabled,schema_hash,record_json,discovered_at
                 FROM mcp_items WHERE server_id=? ORDER BY kind,name",
            )?;
            server.items = items
                .query_map([&server.id], |row| {
                    let record: String = row.get(4)?;
                    Ok(McpItem {
                        kind: row.get(0)?,
                        name: row.get(1)?,
                        enabled: row.get(2)?,
                        schema_hash: row.get(3)?,
                        record: serde_json::from_str(&record).unwrap_or(Value::Null),
                        discovered_at: row.get(5)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            result.push(server);
        }
        Ok(result)
    }

    pub(crate) fn mcp_get(&self, id: &str) -> Result<McpServer, DbError> {
        self.mcp_list()?
            .into_iter()
            .find(|server| server.id == id)
            .ok_or_else(|| DbError::Validation("MCP 服务器不存在".into()))
    }

    fn mcp_save_config(&self, server: &McpServer) -> Result<(), DbError> {
        let connection = self.connect()?;
        connection.execute(
            "INSERT INTO mcp_servers(id,name,transport,config_json,enabled,credential_ref,key_last4,
                status,protocol_version,capabilities_json,instructions,last_error,last_connected_at,
                created_at,updated_at)
             VALUES(?1,?2,?3,?4,?5,?6,?7,'disconnected',NULL,'{}',NULL,NULL,NULL,?8,?9)
             ON CONFLICT(id) DO UPDATE SET name=excluded.name,transport=excluded.transport,
                config_json=excluded.config_json,enabled=excluded.enabled,
                credential_ref=excluded.credential_ref,key_last4=excluded.key_last4,
                status='disconnected',protocol_version=NULL,capabilities_json='{}',instructions=NULL,
                last_error=NULL,last_connected_at=NULL,updated_at=excluded.updated_at",
            params![
                server.id,
                server.name,
                server.transport.kind(),
                serde_json::to_string(&server.transport)?,
                server.enabled,
                server.credential_ref,
                server.key_last4,
                server.created_at,
                server.updated_at,
            ],
        )?;
        connection.execute("DELETE FROM mcp_items WHERE server_id=?", [&server.id])?;
        Ok(())
    }

    fn mcp_save_discovery(&self, id: &str, discovery: &Discovery) -> Result<(), DbError> {
        let mut connection = self.connect()?;
        let transaction = connection.transaction()?;
        let enabled: HashMap<(String, String), bool> = {
            let mut statement =
                transaction.prepare("SELECT kind,name,enabled FROM mcp_items WHERE server_id=?")?;
            let entries = statement
                .query_map([id], |row| Ok(((row.get(0)?, row.get(1)?), row.get(2)?)))?
                .collect::<Result<Vec<_>, _>>()?;
            entries.into_iter().collect()
        };
        transaction.execute("DELETE FROM mcp_items WHERE server_id=?", [id])?;
        let now = chrono::Utc::now().to_rfc3339();
        for (kind, name, raw) in &discovery.items {
            let mut record = raw.clone();
            if kind == "tool" {
                record["local_alias"] = json!(tool_alias(id, name));
            }
            transaction.execute(
                "INSERT INTO mcp_items(server_id,kind,name,enabled,schema_hash,record_json,discovered_at)
                 VALUES(?1,?2,?3,?4,?5,?6,?7)",
                params![
                    id,
                    kind,
                    name,
                    enabled.get(&(kind.clone(), name.clone())).copied().unwrap_or(true),
                    schema_hash(&record),
                    serde_json::to_string(&record)?,
                    now,
                ],
            )?;
        }
        transaction.execute(
            "UPDATE mcp_servers SET status='connected',protocol_version=?2,capabilities_json=?3,
             instructions=?4,last_error=NULL,last_connected_at=?5,updated_at=?5 WHERE id=?1",
            params![
                id,
                discovery.protocol_version,
                serde_json::to_string(&discovery.capabilities)?,
                discovery.instructions,
                now,
            ],
        )?;
        transaction.commit()?;
        Ok(())
    }

    fn mcp_save_error(&self, id: &str, error: &str) -> Result<(), DbError> {
        self.connect()?.execute(
            "UPDATE mcp_servers SET status='error',last_error=?2,updated_at=?3 WHERE id=?1",
            params![
                id,
                error.chars().take(1000).collect::<String>(),
                chrono::Utc::now().to_rfc3339()
            ],
        )?;
        Ok(())
    }

    fn mcp_set_server_enabled_value(&self, id: &str, enabled: bool) -> Result<(), DbError> {
        let changed = self.connect()?.execute(
            "UPDATE mcp_servers SET enabled=?2,updated_at=?3 WHERE id=?1",
            params![id, enabled, chrono::Utc::now().to_rfc3339()],
        )?;
        if changed == 0 {
            return Err(DbError::Validation("MCP 服务器不存在".into()));
        }
        Ok(())
    }

    fn mcp_set_item_enabled_value(
        &self,
        server_id: &str,
        kind: &str,
        name: &str,
        enabled: bool,
    ) -> Result<(), DbError> {
        let changed = self.connect()?.execute(
            "UPDATE mcp_items SET enabled=?4 WHERE server_id=?1 AND kind=?2 AND name=?3",
            params![server_id, kind, name, enabled],
        )?;
        if changed == 0 {
            return Err(DbError::Validation("MCP 能力不存在".into()));
        }
        Ok(())
    }

    fn mcp_delete_value(&self, id: &str) -> Result<Option<String>, DbError> {
        let connection = self.connect()?;
        let credential = connection
            .query_row(
                "SELECT credential_ref FROM mcp_servers WHERE id=?",
                [id],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()?
            .flatten();
        connection.execute("DELETE FROM mcp_servers WHERE id=?", [id])?;
        Ok(credential)
    }

    fn mcp_clear_credential(&self, id: &str) -> Result<Option<String>, DbError> {
        let connection = self.connect()?;
        let credential = connection
            .query_row(
                "SELECT credential_ref FROM mcp_servers WHERE id=?",
                [id],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()?
            .flatten();
        let changed = connection.execute(
            "UPDATE mcp_servers SET credential_ref=NULL,key_last4=NULL,status='disconnected',
             last_error=NULL,updated_at=?2 WHERE id=?1",
            params![id, chrono::Utc::now().to_rfc3339()],
        )?;
        if changed == 0 {
            return Err(DbError::Validation("MCP 服务器不存在".into()));
        }
        Ok(credential)
    }

    pub(crate) fn mcp_snapshots(
        &self,
        server_ids: &[String],
    ) -> Result<Vec<McpServerSnapshot>, DbError> {
        if server_ids.len() > 5 {
            return Err(DbError::Validation(
                "每次复习最多允许 5 个 MCP 服务器".into(),
            ));
        }
        let mut seen = std::collections::HashSet::new();
        let mut snapshots = Vec::with_capacity(server_ids.len());
        let mut tool_count = 0usize;
        for id in server_ids {
            if !seen.insert(id) {
                return Err(DbError::Validation("MCP 服务器选择重复".into()));
            }
            let server = self.mcp_get(id)?;
            if !server.enabled || server.status != "connected" {
                return Err(DbError::Validation(format!(
                    "MCP 服务器 {} 尚未启用并连接成功",
                    server.name
                )));
            }
            let tools = server
                .items
                .iter()
                .filter(|item| item.kind == "tool" && item.enabled)
                .map(|item| {
                    let description = item.record["description"]
                        .as_str()
                        .unwrap_or("外部 MCP 工具")
                        .chars()
                        .take(500)
                        .collect();
                    let input_schema = item.record["inputSchema"].clone();
                    if !input_schema.is_object() || input_schema["type"] != "object" {
                        return Err(DbError::Validation(format!(
                            "MCP Tool {} 的 Schema 无效，请重新测试连接",
                            item.name
                        )));
                    }
                    Ok(McpToolSnapshot {
                        name: item.name.clone(),
                        alias: item.record["local_alias"]
                            .as_str()
                            .map(str::to_owned)
                            .unwrap_or_else(|| tool_alias(&server.id, &item.name)),
                        schema_hash: item.schema_hash.clone(),
                        description,
                        input_schema,
                    })
                })
                .collect::<Result<Vec<_>, DbError>>()?;
            tool_count += tools.len();
            if tool_count > 24 {
                return Err(DbError::Validation(
                    "本次复习启用的 MCP Tools 不能超过 24 个".into(),
                ));
            }
            snapshots.push(McpServerSnapshot {
                id: server.id,
                name: server.name,
                protocol_version: server
                    .protocol_version
                    .unwrap_or_else(|| PROTOCOL_VERSION.into()),
                tools,
            });
        }
        Ok(snapshots)
    }
}

pub(crate) async fn execute_review_tool(
    db: &Database,
    runtime: &McpRuntime,
    secrets: &dyn SecretStore,
    run: &mut ReviewRun,
) -> Result<(), StopReason> {
    let checkpoint = |_| {
        StopReason::new(
            "checkpoint_unavailable",
            "无法保存 MCP 工具检查点，请稍后继续",
            true,
        )
    };
    let call: ToolCall = run.pending[0].clone();
    let Some((snapshot_server, snapshot_tool)) = snapshot_tool(&run.mcp_servers, &call.name)
        .map(|(server, tool)| (server.clone(), tool.clone()))
    else {
        return Err(StopReason::new(
            "unknown_mcp_tool",
            "MCP 工具不在本次会话授权快照中",
            false,
        ));
    };
    run.control.check(run.model_calls, run.tool_calls, false)?;
    run.tool_calls += 1;
    let parsed = (call.arguments.len() <= 32_768)
        .then(|| serde_json::from_str::<Value>(&call.arguments).ok())
        .flatten()
        .filter(Value::is_object);
    let argument_keys = parsed
        .as_ref()
        .and_then(Value::as_object)
        .map(|value| value.keys().take(64).cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    let seq = run.trace_begin(
        "tool",
        &call.name,
        json!({
            "call_id":call.id,
            "transport":"mcp",
            "server_id":snapshot_server.id,
            "remote_tool":snapshot_tool.name,
            "schema_hash":snapshot_tool.schema_hash,
            "argument_keys":argument_keys,
        }),
    );
    let maximum = run.control.policy.tool_timeout_ms.min(30_000);
    let allowance = run.control.reserve_time(maximum);
    db.review_save(run, None).map_err(checkpoint)?;
    let started = Instant::now();

    let outcome: Result<Value, &'static str> = if let Some(arguments) = parsed {
        match db.mcp_get(&snapshot_server.id) {
            Ok(server) if server.enabled => {
                let current = server.items.iter().find(|item| {
                    item.kind == "tool"
                        && item.name == snapshot_tool.name
                        && item.enabled
                        && item.schema_hash == snapshot_tool.schema_hash
                });
                if current.is_none() {
                    Err("工具配置在会话开始后发生变化，请新建复习会话")
                } else {
                    let bearer = server
                        .credential_ref
                        .as_ref()
                        .map(|reference| {
                            secrets
                                .get(reference)
                                .map(|value| value.expose().to_owned())
                        })
                        .transpose();
                    match bearer {
                        Err(_) => Err("MCP 凭据不可用"),
                        Ok(bearer) => match crate::harness::policy::guarded(
                            db,
                            &run.id,
                            allowance,
                            allowance < maximum,
                            runtime.request(
                                &server,
                                bearer,
                                "tools/call",
                                json!({"name":snapshot_tool.name,"arguments":arguments}),
                            ),
                        )
                        .await
                        {
                            Ok(Ok(value)) if value.to_string().len() <= 131_072 => Ok(value),
                            Ok(Ok(_)) => Err("MCP 工具结果超过大小限制"),
                            Ok(Err(_)) => Err("MCP 工具执行失败"),
                            Err(_) => Err("MCP 工具执行超时或已取消"),
                        },
                    }
                }
            }
            Ok(_) => Err("MCP 服务器已停用"),
            Err(_) => Err("MCP 服务器已被删除或不可用"),
        }
    } else {
        Err("MCP 工具参数必须是 JSON object")
    };
    run.control.settle_time(allowance, started.elapsed());
    if db
        .review_load(&run.id)
        .map_err(checkpoint)?
        .cancel_requested
    {
        run.trace_finish(seq, "interrupted");
        return Err(StopReason::cancelled());
    }
    let output = match outcome {
        Ok(data) => json!({"ok":true,"data":data}),
        Err(message) => {
            json!({"ok":false,"error":message,"error_code":"mcp_tool_error","retryable":false})
        }
    };
    run.trace_finish(
        seq,
        if output["ok"] == true {
            "succeeded"
        } else {
            "failed"
        },
    );
    run.trace[seq - 1].details["result"] = json!({
        "ok":output["ok"],
        "bytes":output.to_string().len(),
    });
    run.pending.remove(0);
    run.messages.push(json!({
        "role":"tool",
        "tool_call_id":call.id,
        "content":output.to_string(),
    }));
    let stalled = run.control.observe(&call.name, &call.arguments, &output);
    db.review_save(run, None).map_err(checkpoint)?;
    stalled.map_or(Ok(()), Err)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SaveServerRequest {
    pub id: Option<String>,
    pub name: String,
    pub transport: McpTransportConfig,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub bearer_token: Option<String>,
    #[serde(default)]
    pub replace_existing_credential: bool,
}

fn default_true() -> bool {
    true
}

fn bearer_for(state: &AppState, server: &McpServer) -> Result<Option<String>, String> {
    server
        .credential_ref
        .as_ref()
        .map(|reference| {
            state
                .secrets
                .get(reference)
                .map(|value| value.expose().to_owned())
        })
        .transpose()
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn mcp_list_servers(state: State<'_, AppState>) -> Result<Vec<Value>, String> {
    let _permit = state.persistence_gate.try_operation()?;
    state
        .database
        .mcp_list()
        .map(|servers| servers.iter().map(McpServer::public).collect())
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn mcp_save_server(
    request: SaveServerRequest,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    let _permit = state.persistence_gate.try_operation()?;
    request.transport.validate()?;
    let name = request.name.trim();
    if name.is_empty() || name.chars().count() > 80 {
        return Err("服务器名称需要 1–80 个字符".into());
    }
    let id = request
        .id
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    if !id
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        return Err("MCP 服务器编号无效".into());
    }
    let previous = state.database.mcp_get(&id).ok();
    let token = request
        .bearer_token
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if previous
        .as_ref()
        .is_some_and(|server| server.key_last4.is_some())
        && token.is_some()
        && !request.replace_existing_credential
    {
        return Err(MCP_CREDENTIAL_REPLACEMENT_REQUIRED.into());
    }
    let credential_ref = if token.is_some()
        || previous
            .as_ref()
            .and_then(|s| s.credential_ref.as_ref())
            .is_some()
    {
        Some(format!("mcp:{id}"))
    } else {
        None
    };
    if let Some(value) = token {
        state
            .secrets
            .save(credential_ref.as_deref().unwrap(), value)
            .map_err(|error| error.to_string())?;
    }
    let now = chrono::Utc::now().to_rfc3339();
    let server = McpServer {
        id: id.clone(),
        name: name.into(),
        transport: request.transport,
        enabled: request.enabled,
        credential_ref,
        key_last4: token.map(last_four).or_else(|| {
            previous
                .as_ref()
                .and_then(|server| server.key_last4.clone())
        }),
        status: "disconnected".into(),
        protocol_version: None,
        capabilities: json!({}),
        instructions: None,
        last_error: None,
        last_connected_at: None,
        created_at: previous
            .as_ref()
            .map(|server| server.created_at.clone())
            .unwrap_or_else(|| now.clone()),
        updated_at: now,
        items: vec![],
    };
    state
        .database
        .mcp_save_config(&server)
        .map_err(|error| error.to_string())?;
    state.mcp.disconnect(&id).await;
    Ok(state
        .database
        .mcp_get(&id)
        .map_err(|error| error.to_string())?
        .public())
}

#[tauri::command]
pub async fn mcp_delete_server(id: String, state: State<'_, AppState>) -> Result<(), String> {
    let _permit = state.persistence_gate.try_operation()?;
    let credential = state
        .database
        .mcp_get(&id)
        .map_err(|error| error.to_string())?
        .credential_ref;
    if let Some(reference) = &credential {
        state
            .secrets
            .delete(reference)
            .map_err(|error| error.to_string())?;
    }
    state
        .database
        .mcp_delete_value(&id)
        .map_err(|error| error.to_string())?;
    state.mcp.disconnect(&id).await;
    Ok(())
}

#[tauri::command]
pub async fn mcp_delete_credential(
    id: String,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    let _permit = state.persistence_gate.try_operation()?;
    let reference = state
        .database
        .mcp_clear_credential(&id)
        .map_err(|error| error.to_string())?;
    if let Some(reference) = reference {
        state
            .secrets
            .delete(&reference)
            .map_err(|error| error.to_string())?;
    }
    state.mcp.disconnect(&id).await;
    Ok(state
        .database
        .mcp_get(&id)
        .map_err(|error| error.to_string())?
        .public())
}

#[tauri::command]
pub async fn mcp_test_connection(id: String, state: State<'_, AppState>) -> Result<Value, String> {
    let _permit = state.persistence_gate.try_operation()?;
    let server = state
        .database
        .mcp_get(&id)
        .map_err(|error| error.to_string())?;
    let bearer = bearer_for(&state, &server)?;
    match state.mcp.discover(&server, bearer).await {
        Ok(discovery) => {
            state
                .database
                .mcp_save_discovery(&id, &discovery)
                .map_err(|error| error.to_string())?;
            Ok(state
                .database
                .mcp_get(&id)
                .map_err(|error| error.to_string())?
                .public())
        }
        Err(error) => {
            let message = error.to_string();
            let _ = state.database.mcp_save_error(&id, &message);
            Err(message)
        }
    }
}

#[tauri::command]
pub async fn mcp_set_server_enabled(
    id: String,
    enabled: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let _permit = state.persistence_gate.try_operation()?;
    state
        .database
        .mcp_set_server_enabled_value(&id, enabled)
        .map_err(|error| error.to_string())?;
    if !enabled {
        state.mcp.disconnect(&id).await;
    }
    Ok(())
}

#[tauri::command]
pub fn mcp_set_item_enabled(
    server_id: String,
    kind: String,
    name: String,
    enabled: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let _permit = state.persistence_gate.try_operation()?;
    if !matches!(kind.as_str(), "tool" | "resource" | "prompt") {
        return Err("MCP 能力类型无效".into());
    }
    state
        .database
        .mcp_set_item_enabled_value(&server_id, &kind, &name, enabled)
        .map_err(|error| error.to_string())
}

async fn request_server(
    state: &AppState,
    server_id: &str,
    method: &str,
    params: Value,
) -> Result<Value, String> {
    let server = state
        .database
        .mcp_get(server_id)
        .map_err(|error| error.to_string())?;
    if !server.enabled {
        return Err("该 MCP 服务器已停用".into());
    }
    let bearer = bearer_for(state, &server)?;
    state
        .mcp
        .request(&server, bearer, method, params)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn mcp_call_tool(
    server_id: String,
    name: String,
    arguments: Value,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    let _permit = state.persistence_gate.try_operation()?;
    let server = state
        .database
        .mcp_get(&server_id)
        .map_err(|error| error.to_string())?;
    if !server
        .items
        .iter()
        .any(|item| item.kind == "tool" && item.name == name && item.enabled)
    {
        return Err("该 MCP 工具不存在或已停用".into());
    }
    if !arguments.is_object() || arguments.to_string().len() > 65_536 {
        return Err("MCP 工具参数无效或过长".into());
    }
    request_server(
        &state,
        &server_id,
        "tools/call",
        json!({"name":name,"arguments":arguments}),
    )
    .await
}

#[tauri::command]
pub async fn mcp_read_resource(
    server_id: String,
    uri: String,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    let _permit = state.persistence_gate.try_operation()?;
    let server = state
        .database
        .mcp_get(&server_id)
        .map_err(|error| error.to_string())?;
    if !server
        .items
        .iter()
        .any(|item| item.kind == "resource" && item.name == uri && item.enabled)
    {
        return Err("该 MCP 资源不存在或已停用".into());
    }
    request_server(&state, &server_id, "resources/read", json!({"uri":uri})).await
}

#[tauri::command]
pub async fn mcp_get_prompt(
    server_id: String,
    name: String,
    arguments: Value,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    let _permit = state.persistence_gate.try_operation()?;
    let server = state
        .database
        .mcp_get(&server_id)
        .map_err(|error| error.to_string())?;
    if !server
        .items
        .iter()
        .any(|item| item.kind == "prompt" && item.name == name && item.enabled)
    {
        return Err("该 MCP Prompt 不存在或已停用".into());
    }
    request_server(
        &state,
        &server_id,
        "prompts/get",
        json!({"name":name,"arguments":arguments}),
    )
    .await
}

#[tauri::command]
pub async fn mcp_logs(id: String, state: State<'_, AppState>) -> Result<Vec<String>, String> {
    let _permit = state.persistence_gate.try_operation()?;
    state
        .database
        .mcp_get(&id)
        .map_err(|error| error.to_string())?;
    Ok(state.mcp.logs(&id).await)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct ScriptedPeer {
        replies: VecDeque<(&'static str, Value)>,
        notified: bool,
    }

    #[async_trait]
    impl RpcPeer for ScriptedPeer {
        async fn request(&mut self, method: &str, _params: Value) -> Result<Value, McpError> {
            let (expected, value) = self.replies.pop_front().expect("unexpected request");
            assert_eq!(method, expected);
            Ok(value)
        }

        async fn notify(&mut self, method: &str, _params: Value) -> Result<(), McpError> {
            assert_eq!(method, "notifications/initialized");
            self.notified = true;
            Ok(())
        }

        async fn logs(&self) -> Vec<String> {
            vec![]
        }
    }

    #[tokio::test]
    async fn discovery_reads_tools_resources_and_prompts() {
        let mut peer = ScriptedPeer {
            replies: VecDeque::from([
                (
                    "initialize",
                    json!({"protocolVersion":PROTOCOL_VERSION,"capabilities":{"tools":{},"resources":{},"prompts":{}},"instructions":"test only"}),
                ),
                (
                    "tools/list",
                    json!({"tools":[{"name":"echo","inputSchema":{"type":"object"}}]}),
                ),
                (
                    "resources/list",
                    json!({"resources":[{"uri":"test://one","name":"one"}]}),
                ),
                ("prompts/list", json!({"prompts":[{"name":"hello"}]})),
            ]),
            notified: false,
        };
        let result = initialize_and_discover(&mut peer).await.unwrap();
        assert!(peer.notified);
        assert_eq!(result.items.len(), 3);
        assert_eq!(result.instructions.as_deref(), Some("test only"));
    }

    #[tokio::test]
    async fn stdio_transport_discovers_and_calls_a_real_child_process() {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("mcp-stdio-server.mjs");
        let config = McpTransportConfig::Stdio {
            command: "node".into(),
            args: vec![fixture.to_string_lossy().into_owned()],
            cwd: env!("CARGO_MANIFEST_DIR").into(),
        };
        let mut peer = StdioPeer::connect(&config).await.unwrap();
        let discovery = initialize_and_discover(&mut peer).await.unwrap();
        assert_eq!(discovery.items.len(), 3);
        let result = peer
            .request(
                "tools/call",
                json!({"name":"echo","arguments":{"value":"stdio-ok"}}),
            )
            .await
            .unwrap();
        assert_eq!(result["content"][0]["text"], "stdio-ok");
    }

    #[tokio::test]
    async fn review_tool_executor_uses_the_session_snapshot_and_stdio_transport() {
        use crate::{
            providers::ProviderContext,
            review_agent::{ReviewRun, ReviewScope},
            secret_store::tests_support::MemorySecretStore,
        };

        let directory = tempfile::tempdir().unwrap();
        let database = Database::new(directory.path().join("agent-mcp.sqlite"));
        database.initialize().unwrap();
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("mcp-stdio-server.mjs");
        let now = chrono::Utc::now().to_rfc3339();
        let server = McpServer {
            id: "agent-stdio".into(),
            name: "Agent stdio".into(),
            transport: McpTransportConfig::Stdio {
                command: "node".into(),
                args: vec![fixture.to_string_lossy().into_owned()],
                cwd: env!("CARGO_MANIFEST_DIR").into(),
            },
            enabled: true,
            credential_ref: None,
            key_last4: None,
            status: "disconnected".into(),
            protocol_version: None,
            capabilities: json!({}),
            instructions: None,
            last_error: None,
            last_connected_at: None,
            created_at: now.clone(),
            updated_at: now,
            items: vec![],
        };
        database.mcp_save_config(&server).unwrap();
        let runtime = McpRuntime::new().unwrap();
        let discovery = runtime.discover(&server, None).await.unwrap();
        database.mcp_save_discovery(&server.id, &discovery).unwrap();

        let context =
            ProviderContext::from_registry("deepseek", "default", "deepseek-v4-flash").unwrap();
        let mut created = ReviewRun::new(
            ReviewScope {
                kb: "book".into(),
                version: "v1".into(),
                filename: "Book".into(),
                chapter: None,
                chapter_path: vec![],
            },
            "test MCP".into(),
            &context,
            "deepseek",
            "default",
        );
        created.mcp_servers = database.mcp_snapshots(&[server.id.clone()]).unwrap();
        let alias = created.mcp_servers[0].tools[0].alias.clone();
        database.review_insert(&created).unwrap();
        let mut run = database.review_claim(&created.id).unwrap();
        run.pending.push(ToolCall {
            id: "call-mcp".into(),
            name: alias,
            arguments: r#"{"value":"agent-loop-ok"}"#.into(),
        });
        database.review_save(&run, None).unwrap();

        execute_review_tool(&database, &runtime, &MemorySecretStore::default(), &mut run)
            .await
            .unwrap();

        assert!(run.pending.is_empty());
        assert_eq!(run.tool_calls, 1);
        assert!(run.messages.last().unwrap()["content"]
            .as_str()
            .unwrap()
            .contains("agent-loop-ok"));
        let event = run.trace.last().unwrap();
        assert_eq!(event.details["transport"], "mcp");
        assert_eq!(event.details["argument_keys"], json!(["value"]));
        assert!(!event.details.to_string().contains("agent-loop-ok"));
    }

    #[tokio::test]
    #[ignore = "requires network access to the public OpenAI Docs MCP server"]
    async fn openai_docs_streamable_http_smoke() {
        let now = chrono::Utc::now().to_rfc3339();
        let server = McpServer {
            id: "openai-docs".into(),
            name: "OpenAI Docs".into(),
            transport: McpTransportConfig::StreamableHttp {
                url: "https://developers.openai.com/mcp".into(),
                proxy_url: std::env::var("TELLWHY_MCP_TEST_PROXY").ok(),
            },
            enabled: true,
            credential_ref: None,
            key_last4: None,
            status: "disconnected".into(),
            protocol_version: None,
            capabilities: json!({}),
            instructions: None,
            last_error: None,
            last_connected_at: None,
            created_at: now.clone(),
            updated_at: now,
            items: vec![],
        };
        let runtime = McpRuntime::new().unwrap();
        let discovery = runtime.discover(&server, None).await.unwrap();
        assert!(discovery.items.iter().any(|(kind, _, _)| kind == "tool"));
        assert!(!discovery.protocol_version.is_empty());
    }

    #[test]
    fn parses_sse_and_rejects_unsafe_remote_urls() {
        let value = parse_sse_response(
            "event: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"ok\":true}}\n\n",
        )
        .unwrap();
        assert_eq!(rpc_result(value).unwrap()["ok"], true);
        assert!(validate_http_url("https://developers.openai.com/mcp").is_ok());
        assert!(validate_http_url("http://127.0.0.1:8765/mcp").is_ok());
        assert!(validate_http_url("http://example.com/mcp").is_err());
        assert!(validate_http_url("file:///tmp/server").is_err());
        assert!(validate_proxy_url("http://127.0.0.1:7897").is_ok());
        assert!(validate_proxy_url("http://localhost:7897").is_ok());
        assert!(validate_proxy_url("http://example.com:7897").is_err());
        assert!(validate_proxy_url("socks5://127.0.0.1:7897").is_err());
    }

    #[test]
    fn aliases_are_stable_bounded_and_logs_are_redacted() {
        let first = tool_alias("server-one", "search/docs");
        assert_eq!(first, tool_alias("server-one", "search/docs"));
        assert!(first.len() <= 64);
        assert!(tool_alias("x", &"long".repeat(40)).len() <= 64);
        assert!(!redact_log("Authorization: Bearer secret-value").contains("secret-value"));
    }

    #[test]
    fn database_persists_discovery_without_credentials() {
        let directory = tempfile::tempdir().unwrap();
        let database = Database::new(directory.path().join("mcp.sqlite"));
        database.initialize().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        let server = McpServer {
            id: "test-server".into(),
            name: "Test".into(),
            transport: McpTransportConfig::StreamableHttp {
                url: "https://example.com/mcp".into(),
                proxy_url: None,
            },
            enabled: true,
            credential_ref: Some("mcp:test-server".into()),
            key_last4: Some("1234".into()),
            status: "disconnected".into(),
            protocol_version: None,
            capabilities: json!({}),
            instructions: None,
            last_error: None,
            last_connected_at: None,
            created_at: now.clone(),
            updated_at: now,
            items: vec![],
        };
        database.mcp_save_config(&server).unwrap();
        database
            .mcp_save_discovery(
                &server.id,
                &Discovery {
                    protocol_version: PROTOCOL_VERSION.into(),
                    capabilities: json!({"tools":{}}),
                    instructions: None,
                    items: vec![(
                        "tool".into(),
                        "echo".into(),
                        json!({"name":"echo","inputSchema":{"type":"object"}}),
                    )],
                },
            )
            .unwrap();
        let loaded = database.mcp_get(&server.id).unwrap();
        assert_eq!(loaded.items.len(), 1);
        assert_eq!(
            loaded.items[0].record["local_alias"],
            "mcp__test_server__echo"
        );
        let database_bytes = std::fs::read(directory.path().join("mcp.sqlite")).unwrap();
        assert!(!String::from_utf8_lossy(&database_bytes).contains("secret-value"));

        let mut without_credential = server.clone();
        without_credential.id = "without-credential".into();
        without_credential.credential_ref = None;
        without_credential.key_last4 = None;
        database.mcp_save_config(&without_credential).unwrap();
        assert_eq!(
            database.mcp_delete_value(&without_credential.id).unwrap(),
            None
        );
        assert_eq!(
            database.mcp_clear_credential(&server.id).unwrap(),
            Some("mcp:test-server".into())
        );
        assert!(database
            .mcp_get(&server.id)
            .unwrap()
            .credential_ref
            .is_none());
    }
}
