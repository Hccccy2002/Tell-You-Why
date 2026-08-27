# P0 安全与隐私复核

状态：代码级复核、Rust 严格检查、Windows 凭据 Mock 实机往返和安装包构建已完成；真实供应商账户、Windows 交互与发布签名仍需人工执行。

## API Key

- 完整 Key 只作为 Tauri 命令参数短暂进入 Rust 核心。
- 前端密码输入框禁用自动完成；提交保存时先清空 React 状态，即使系统凭据写入失败也不继续保留原文。
- Rust 使用 keyring 的 Windows 原生后端保存到 Windows Credential Manager。
- SQLite provider_profiles 只保存凭据引用、最后四位、供应商、区域、模型和开关。
- SecretValue 的 Debug 输出固定为 [REDACTED]。
- 供应商错误只返回预定义中文消息，不回传响应体或鉴权头。
- 删除配置与“清除全部数据”先删除系统凭据，再删除数据库引用。
- 仓库没有真实 Key；测试只使用明显的 Mock 字符串。

## 网络

- 所有模型 HTTP 从 Rust 发起，前端 CSP 的 connect-src 只允许本地 IPC。
- Provider Registry 只包含 api.deepseek.com、api.moonshot.cn 和 api.moonshot.ai。
- URL 检查要求 HTTPS、标准 443 端口、无用户名/密码、精确白名单主机。
- HTTP 客户端关闭自动重定向，避免携带 Key 跳转到其他域名。
- 普通设置没有 Base URL 输入。

## WebView 与内容

- WebView 只加载本地打包资源。
- CSP 禁止远程脚本、对象、框架和表单提交。
- Tauri capability 仅给主窗口核心 IPC 和文件选择权限。
- 前端没有通用文件系统或 Shell 插件。
- 知识正文全部用 React 纯文本节点渲染，不渲染模型 HTML 或 Markdown。
- 来源链接仅接受 HTTPS，打开动作由 Rust 再次校验。
- 导入限制为 JSON/CSV 和 5 MB，并进行字段、来源、风险主题和重复校验。

## 已完成的实机/构建确认

- Windows Credential Manager 使用随机 Mock 凭据完成保存、读取、删除，并确认删除后不可再读。
- Tauri 开发版启动成功，窗口句柄有效、进程持续响应，首次启动创建 SQLite 数据库。
- Release EXE、NSIS 和 MSI 构建成功；产物哈希已在实施计划执行记录中核验。
- npm 生产依赖审计为 0 个已知漏洞。

## 尚需人工确认

- 设置页中的 Key 掩码显示、删除配置和“全部清除”完整 UI 流程。
- 无效 Key、余额不足、429、超时的真实供应商错误映射。
- NSIS/MSI 安装、升级、卸载及企业策略环境表现。
- Windows 通知点击行为和企业策略环境。
- 发布代码签名、SmartScreen 信誉和签名更新。
