# Tell You Why

Tell You Why 是一款运行在 Windows 10/11 上的轻量知识小窗，适合在办公间隙用几十秒了解一个“为什么”。

应用默认窗口为 420 × 560 像素，最小尺寸为 360 × 440。即使没有网络、没有注册账号、没有配置 API Key，用户也可以浏览随应用提供的本地演示知识卡。

> 当前版本是 P0 MVP 开发版。内置知识卡统一标记为“演示内容 · 未经人工复核”，不能作为正式发布的内容包。公开发布前仍需导入并人工复核 100–300 条正式内容。

## 当前能力

### 知识浏览

- 首次使用引导、预设兴趣和自定义兴趣。
- 问题思考、揭晓答案、详细解释、上一条和下一条。
- 收藏、不感兴趣和“已狠狠涨知识”。
- 被归档的知识卡不会再次出现在主推荐流，但会保留在“收藏与历史 → 最近浏览”。
- 最近浏览支持逐条删除、二次确认和一键清空浏览记录。
- 本地内容不依赖模型服务，断网或模型故障不会阻断浏览。

### 兴趣与生成

- 兴趣列表支持鼠标长按拖拽和一键置顶。
- 随机领域生成按照兴趣列表的实时顺序分配权重，越靠上，被选中的概率越高。
- 主界面可指定领域、输入自定义领域或按兴趣权重随机选择领域。
- 用户可以自定义本次生成数量和每日生成总数，并查看“已生成数/总数”。
- 生成任务会按供应商能力自动分批；部分批次失败时保留已经成功入库的知识卡，并允许继续生成剩余数量。
- 所有模型内容都标记为“AI 生成 · 未经外部核验”。

### 模型与故障切换

- 支持 DeepSeek 和 Kimi。
- 主界面可以选择首选生成模型，选择结果保存在 SQLite，重启后继续生效。
- 只有已经配置 Key 且通过连接测试的模型才会参与生成。
- 首选模型未就绪时，应用会直接使用另一个已就绪模型。
- 首选模型生成失败时，应用最多自动尝试另一个模型一次；两个模型都失败后才提示用户。
- 批量生成发生切换时，已成功内容不会丢失，界面会显示实际使用的模型和切换结果。
- 应用不会后台自动生成，只有用户主动点击生成按钮时才会请求模型。

自动切换可能使一次用户操作同时请求两家供应商，并可能分别产生费用。应用不会循环切换或在后台反复重试，建议使用独立、低额度、可吊销的 API Key。

### Windows 桌面体验

- 系统托盘、关闭到托盘和显示/隐藏。
- 默认全局快捷键：`Alt + Shift + Y`。
- 窗口位置、尺寸和显示器恢复。
- 置顶、开机启动、浅色/深色/跟随系统主题。
- 工作日提醒、静默时段、暂停提醒和全屏应用检测。

### 本地数据与内容导入

- SQLite 数据库和版本化迁移。
- 收藏、历史、兴趣、设置和模型首选项持久化。
- JSON/CSV 知识卡导入，支持字段校验、来源、可信状态、高风险主题过滤和重复检测。
- 人工导入内容不受 AI 动态缓存上限影响。
- AI 生成内容只保留最近 30 张动态缓存卡。

## 技术栈

| 层级       | 实现                                         |
| ---------- | -------------------------------------------- |
| 桌面框架   | Tauri 2                                      |
| 前端       | React 19、TypeScript 5.9、Vite 7             |
| 核心能力   | Rust                                         |
| 本地数据库 | SQLite（rusqlite bundled）                   |
| 系统凭据   | Windows Credential Manager                   |
| 网络请求   | Rust reqwest，禁用自动重定向并限制官方白名单 |
| 前端测试   | Vitest、Testing Library                      |

所有供应商 API 请求都由 Rust 核心发起。前端不能直接访问供应商接口，也不会持久化完整 API Key。

## 快速启动

### 方式一：启动已经构建好的程序

如果项目中已经存在 Release 程序，直接双击根目录的：

`启动 Tell You Why.cmd`

该脚本只启动：

`src-tauri\target\release\tell-you-why.exe`

它不会安装依赖、启动开发服务器或重新编译。若 Release 程序尚未生成，脚本会提示先执行构建。

### 方式二：安装 Windows 安装包

当前构建产物位于：

- `src-tauri\target\release\bundle\nsis\Tell You Why_0.1.0_x64-setup.exe`
- `src-tauri\target\release\bundle\msi\Tell You Why_0.1.0_x64_en-US.msi`

当前安装包未进行代码签名，仅适合开发和封闭测试。Windows SmartScreen 可能显示未知发布者提示。

## 开发环境

### 环境要求

- Windows 10 1803 或更高版本，或 Windows 11。
- Node.js 20 或更高版本。
- npm 10 或更高版本。
- Rust stable，MSVC toolchain。
- Microsoft Visual Studio 2022 Build Tools。
- WebView2 Runtime。

安装 Visual Studio Build Tools 时需要勾选“使用 C++ 的桌面开发”。Build Tools 主体可以安装在 D 盘；Windows SDK 和部分共享组件仍可能由微软安装器放在系统盘，项目不依赖固定安装路径。

如果 PowerShell 阻止执行 `npm.ps1`，直接使用下文的 `npm.cmd`，不需要修改系统执行策略。

### 安装依赖

在项目根目录 `D:\Tell You Why` 打开 PowerShell：

```powershell
npm.cmd install
```

首次构建 Rust 部分时，Cargo 会下载并编译依赖。

### 启动桌面开发模式

```powershell
npm.cmd run tauri -- dev
```

### 仅启动前端预览

```powershell
npm.cmd run dev
```

前端开发地址为 `http://127.0.0.1:1420`。浏览器模式使用内存演示数据，不支持 SQLite 持久化、系统凭据、托盘、通知和全局快捷键，因此不能代替桌面模式验收。

## 模型配置

模型配置是可选项。本地知识浏览不需要 API Key。

### 内置供应商

| 供应商   | 区域   | 可选模型                               | 允许访问的官方域名 |
| -------- | ------ | -------------------------------------- | ------------------ |
| DeepSeek | 默认   | `deepseek-v4-flash`、`deepseek-v4-pro` | `api.deepseek.com` |
| Kimi     | 中国区 | `kimi-k3`、`kimi-k2.6`                 | `api.moonshot.cn`  |
| Kimi     | 全球区 | `kimi-k3`、`kimi-k2.6`                 | `api.moonshot.ai`  |

Base URL 和模型列表由 Rust 内置 Provider Registry 管理，普通用户不能输入任意服务器地址。

### 配置步骤

1. 打开“模型设置”。
2. 选择 DeepSeek 或 Kimi，以及应用提供的区域和模型。
3. 输入 API Key 并保存。
4. 等待输入框清空，确认界面只显示 Key 的最后四位。
5. 点击连接测试。
6. 返回知识主界面，选择首选模型后开始生成。

不要将真实 API Key 写入 `.env`、命令行、SQLite、普通配置文件、日志、测试代码或聊天消息。

## 当前生成与去重逻辑

生成请求只向模型提供领域、生成数量、字段格式和安全边界，不会把完整历史题目列表发送给供应商。

模型返回后，本地会执行：

1. JSON 结构和字段范围校验。
2. 高风险主题过滤。
3. 与数据库已有问题及本批已接收问题比较。
4. 跳过被判定为重复的知识卡。
5. 保存通过校验的内容，并标记为未经核验的 AI 内容。

当前重复检测以规范化题面的连续双字符集合为基础，相似度达到 88% 才会判定重复。它可以阻止完全相同或几乎相同的问题，但仍可能漏过语义相同、措辞不同的问题，例如“IPv4 和 IPv6 有什么区别”与“IPv4 和 IPv6 的主要差异是什么”。

这是当前 MVP 的已知限制：应用尚未实现语义向量去重或稳定的“知识主题键”。AI 卡超过最近 30 张被缓存淘汰，或者用户彻底删除某张 AI 卡后，该问题也可能在未来再次生成。

## 数据与隐私

默认 SQLite 数据库位置：

```text
%APPDATA%\com.tellyouwhy.desktop\tell-you-why.db
```

API Key 保存在 Windows Credential Manager。SQLite 只保存供应商配置、凭据引用和最后四位掩码，不保存完整 Key。

- “清除阅读记录”只删除浏览交互，不删除收藏内容。
- “清除偏好”重置本地兴趣权重并删除自定义兴趣。
- “清除全部本地数据”删除非内置内容、设置和供应商配置，并同时清理相应系统凭据。

错误信息和日志不得包含完整 API Key、Authorization 请求头或未经处理的供应商响应正文。

## 内容导入

设置页支持最大 5 MB 的 JSON 或 CSV 文件。正式内容应先由人工编写、核验来源和复核可信状态，再通过应用导入。

- [内容导入与审核流程](docs/content-import.md)
- [JSON 导入模板](examples/cards-import-template.json)
- [CSV 导入模板](examples/cards-import-template.csv)

未经人工审核的 AI 内容不得标记为“人工复核”或“已核验”。

## 测试与静态检查

```powershell
npm.cmd run format
npm.cmd run lint
npm.cmd run test
npm.cmd run build

cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
```

最近一次验证结果：

- 前端测试 18 项通过。
- Rust 常规测试 32 项通过。
- 1 项 Windows Credential Manager 实机测试默认忽略，需要人工显式执行。
- Prettier、ESLint、TypeScript、Vite 生产构建和严格 Clippy 通过。
- DeepSeek、Kimi 适配器以及跨供应商故障切换均使用本地 Mock Transport 测试，不读取真实 Key、不访问真实供应商、不产生费用。

## 构建 Windows 程序

```powershell
npm.cmd run tauri -- build
```

构建完成后会生成：

```text
src-tauri\target\release\tell-you-why.exe
src-tauri\target\release\bundle\nsis\Tell You Why_0.1.0_x64-setup.exe
src-tauri\target\release\bundle\msi\Tell You Why_0.1.0_x64_en-US.msi
```

公开发布前必须配置 Windows 代码签名，并重新验证安装、升级、卸载以及 SmartScreen 行为。

## 项目结构

```text
src/                         React 前端、页面、组件和前端测试
src-tauri/src/               Rust 命令、SQLite、桌面能力和模型适配器
src-tauri/resources/         随应用提供的演示知识卡
src-tauri/capabilities/      Tauri 权限配置
docs/                        实施计划、技术决策、安全与人工验收文档
examples/                    JSON/CSV 内容导入模板
启动 Tell You Why.cmd        Release 快捷启动脚本
```

## 仍需人工完成

- 准备并复核 100–300 条正式知识卡内容。
- 使用低额度 DeepSeek 和 Kimi 真实账户分别测试连接、单批生成、批量生成和自动故障切换。
- 在 420 × 560 与 360 × 440 真实窗口中复核布局和键盘操作。
- 验收系统托盘、快捷键冲突、通知、开机启动、窗口恢复和多显示器拔插。
- 验收 NSIS/MSI 的安装、升级和卸载。
- 配置 Windows 代码签名证书并进行安全软件抽检。

完整项目状态请查看 [MVP 实施计划](docs/implementation-plan.md) 和 [Windows 人工测试清单](docs/manual-test-checklist.md)。

## 项目文档

- [原始 MVP 设计方案](Tell%20You%20Why%20-%20MVP设计方案.md)
- [MVP 实施计划与 P0 追踪](docs/implementation-plan.md)
- [技术决策记录](docs/decisions.md)
- [内容导入与审核流程](docs/content-import.md)
- [安全复核](docs/security-review.md)
- [Windows 人工测试清单](docs/manual-test-checklist.md)

原始设计方案是产品范围、功能要求和验收标准的主要依据，开发过程中保持原样。
