# Tell You Why

> Windows x64 完整版 0.1.1：EXE 安装包包含 Python、PDF/OCR 依赖、五组本地模型和 WebView2 离线安装组件。无需先安装 Node.js、Rust 或 Python。仓库保持私有，下载需要有权限的 GitHub 账号。

Tell You Why 是一款运行在 Windows 10 1903+ / Windows 11 上的轻量知识小窗，适合在办公间隙用几十秒了解一个“为什么”。

应用默认窗口为 420 × 560 像素，最小尺寸为 360 × 440。即使没有网络、没有注册账号、没有配置 API Key，用户也可以浏览随应用提供的本地演示知识卡。

> 当前版本是 P0 MVP 开发版。内置知识卡统一标记为“演示内容 · 未经人工复核”，不能作为正式发布的内容包。公开发布前仍需导入并人工复核 100–300 条正式内容。

## 安装与一键启动

### 方式一：下载 EXE 安装程序

1. 登录有仓库访问权限的 GitHub 账号，打开 [v0.1.1 下载页](https://github.com/Hccccy2002/Tell-You-Why/releases/tag/v0.1.1)。
2. 下载 **`Tell-You-Why_0.1.1_windows-x64-full-setup.exe`**，双击安装，然后从开始菜单打开 Tell You Why。
3. 打开 **PDF 知识库 → 选择 PDF → 开始导入**。首次安装没有用户资料，需要自行导入。

完整包约 **1.75 GB**，支持 Windows 10 1903+ / Windows 11 x64，建议预留 **10 GB** 可用空间用于下载、解包及安装，导入 PDF 另需空间。PDF 处理和本地检索可离线运行；AI 解释、生成和复习仍需在 APP 中配置模型通道及联网。安装包未签名，Windows 可能显示未知发布者；只下载本仓库 Release，并可对照同页 `SHA256SUMS.txt` 校验。

### 方式二：下载源码后执行一条命令

解压源码，在项目根目录执行：

```powershell
.\start.cmd
```

也可以双击 **`start.cmd`** 或 **`启动 Tell You Why.cmd`**。脚本下载该源码版本固定的完整安装包，校验 SHA256，安装到当前用户的 `%LOCALAPPDATA%\Programs\Tell You Why`，随后启动。中断后已完整下载且校验一致的文件可复用；以后再次运行直接启动已安装版本。需要安装或升级时，请先从托盘菜单退出正在运行的 APP。**此入口运行已发布的程序；修改源码后请用下文开发模式。**

私有仓库首次下载需认证，任选一种：

- 已用 Git Credential Manager 登录且有仓库权限：脚本复用该登录。
- 使用 GitHub CLI：先执行 `gh auth login`，再运行 `start.cmd`。若未安装 GitHub CLI，可执行 `winget install --id GitHub.cli --exact`，重开终端后登录。
- 不安装 Git / GitHub CLI：用浏览器登录并下载上述 EXE，放在 `start.cmd` 同一目录，再双击 `start.cmd`。文件名须保持不变；同样会校验。

脚本也支持已有的 `GH_TOKEN` / `GITHUB_TOKEN` 环境变量，不会把令牌保存到项目文件或转发到下载 CDN。没有认证时会明确提示；不会更改仓库可见性。诊断启动配置可运行 `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\start.ps1 -CheckOnly`。

安装版的 PDF 数据保存在 `%LOCALAPPDATA%\com.tellyouwhy.desktop\pdf-data`，应用数据库仍在 `%APPDATA%\com.tellyouwhy.desktop`。安装版和源码开发版默认使用不同的 PDF 数据目录，旧项目的知识库不会自动迁移。安装包不包含个人 PDF、知识库、API Key 或账号登录信息。

## 当前能力

### 知识浏览

- 首次使用引导、预设兴趣和自定义兴趣。
- 问题思考、揭晓答案、详细解释、上一条和下一条。
- 收藏、不感兴趣和“已狠狠涨知识”。
- 被归档的知识卡不会再次出现在主推荐流，但会保留在“收藏与历史 → 最近浏览”。
- 最近浏览支持逐条删除和清空浏览记录，两种操作都需要二次确认。
- 本地内容不依赖模型服务，断网或模型故障不会阻断浏览。

### AI 追问与选词查询

- 揭晓答案后，可以在“继续追问”中结合当前卡片向模型提问。问题最多 500 字，一次请求会携带当前卡片内容、当前问题和最近 6 条对话记录。
- 成功生成的追问与 AI 回复会按知识卡保存在 SQLite 中；离开当前页面、切换知识卡或重启应用后，重新打开同一张知识卡仍会恢复完整追问线程。
- 在简短答案、详细解释或“为什么值得知道”中选择同一文本块内、不超过 200 个字符的文字，会在选区附近显示“让 AI 解释这段？”确认框；只有点击“AI 解释”后才会发起请求。
- 确认框会说明所选文字、当前卡片上下文将被发送给模型且可能产生费用；所选文字只被当作待解释的数据，不会被当作系统指令执行。
- 选词查询复用当前卡片的追问上下文，同时保留输入框中尚未发送的问题草稿。请求失败后会保留内容，并由用户明确选择是否重试。
- 回复会显示实际作答的供应商、模型和故障切换结果，并统一标记为 AI 未核验内容。
- 回复使用受限 Markdown 渲染，不执行原始 HTML；只有 HTTPS 链接可点击，图片不会直接加载。
- 长回复限制在对话区域内滚动；代码块和宽表格可在回复内部横向滚动，不会撑破知识小窗。
- 模型回复期间会暂时锁定切换卡片等可能丢失当前上下文的导航操作。

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
- “生成模型”下拉框会显示“已就绪”“待连接测试”或“未配置”。当前通道未配置时显示“去配置”，已有 Key 但尚未验证时显示“去测试”，均可直接进入“模型设置”；通道就绪后入口自动隐藏。
- 只有已经配置 Key 且通过连接测试的模型才会参与生成。
- 首选模型未就绪时，应用会直接使用另一个已就绪模型。
- 首选模型生成失败时，应用最多自动尝试另一个模型一次；两个模型都失败后才提示用户。
- 批量生成发生切换时，已成功内容不会丢失，界面会显示实际使用的模型和切换结果。
- 卡片追问与选词查询同样遵循就绪状态和一次跨供应商故障切换。
- 应用不会后台自动生成，只有用户主动点击生成按钮时才会请求模型。

自动切换可能使一次用户操作同时请求两家供应商，并可能分别产生费用。应用不会循环切换或在后台反复重试，建议使用独立、低额度、可吊销的 API Key。

### Windows 桌面体验

- 系统托盘、关闭到托盘和显示/隐藏。
- 默认全局快捷键：`Alt + Shift + Y`。
- 窗口位置、尺寸和显示器恢复。
- 置顶、开机启动、浅色/深色/跟随系统主题。
- 可选“鼠标移出后自动收起”：鼠标先进入窗口、再持续离开约 0.7 秒后收至系统托盘；快速移回、弹窗、菜单、文件选择器、拖选和窗口移动/缩放不会误触发。
- 自动收起不会退出应用或取消正在进行的 AI 请求；托盘和全局快捷键可恢复原页面。首次自动收起会显示一次恢复方式提示。
- 工作日提醒、静默时段、暂停提醒和全屏应用检测。

### 本地数据与内容导入

- SQLite 数据库和版本化迁移。
- 收藏、历史、兴趣、设置、模型首选项和每张知识卡的追问线程均持久化。
- JSON/CSV 知识卡导入，支持字段校验、来源、可信状态、高风险主题过滤和重复检测。
- 人工导入内容不受 AI 动态缓存上限影响。
- 最近浏览最多保留 500 条记录。
- AI 动态缓存只淘汰最近 30 张之外、且从未发生用户交互的缓存托管卡片；已读、收藏、反馈或归档过的卡片不会被缓存清理误删，因此 AI 卡总数可以超过 30。
- 清除数据、删除记录或兴趣、移除或覆盖 API Key，以及将知识卡永久移出推荐流等敏感操作，都会先显示应用内确认对话框。

## 技术栈

复习执行已升级到 [Harness v2](docs/harness-v2-guide.md)：五个工具统一参数、权限、错误和重试规则；暂停与超时会清理本次资料调用的 Python 进程组，并拒绝迟到结果。前两阶段的测试与构建结果见 [v2 验收记录](docs/harness-v2-development.md)。

PDF 随机学习与复习 Agent 的“相关原文”按当前题目检索并重排，只展示最多 5 条。使用本地 BGE Reranker，无额外 LLM API 调用。模型准备命令见 [Python 模块说明](rag-service/README.md#安装与模型准备)，方法和实测结果见 [Top 5 原文检索](docs/related-sources-top5.md)。

PDF 资料准备的 Python 模块位于 `rag-service/`，已接入桌面 APP。点击右上角 **PDF** 或菜单中的 **PDF 知识库**，即可导入、暂停续跑、检索和查看原文。已有资料只在本机保留了对应知识库数据时显示，首次使用需先导入 PDF。“随机学习”提供简洁知识卡、直接生成新卡、原文对照和模型解释。“复习 Agent”根据学习记录调用工具检索教材、讲解、出题并记录用户实际答题结果，支持到期复习、失败恢复与执行追踪导出。新增 [Harness v1](docs/harness-v1-guide.md) 管理运行预算、有限重试、上下文组装和完成验收，可指定题数及教材依据要求。使用方式见 [复习 Agent](docs/review-agent-guide.md)，质量评测见 [评测说明](evals/review-agent/README.md)，故障注入见 [Harness 评测](evals/harness/README.md)。安装和排错直接见下文“PDF 知识库运行环境”；界面操作见 [知识库界面](docs/pdf-desktop-guide.md)，详细命令见 [Python 模块说明](rag-service/README.md)。

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

## PDF 知识库运行环境

**完整安装包用户无需执行本节。** 本节用于源码开发或自定义部署。若完整安装版提示组件缺失，请先检查安装目录是否保留 `pdf-runtime/`，必要时重新安装完整包；不要只复制主 EXE。

### 运行要求

| 使用方式                               | 需要准备                                                 |
| -------------------------------------- | -------------------------------------------------------- |
| 浏览内置知识卡                         | Windows / WebView2；不需要 Python 或模型 API Key         |
| PDF 导入、OCR、原文浏览与本地检索      | Python 3.12 x64 独立环境、锁定依赖、本地 OCR 与 BGE 模型 |
| PDF“相关原文”Top 5                     | 上述环境，以及单独的 BGE Reranker 模型                   |
| 教材 AI 解释、随机学习生成、复习 Agent | 可用的 PDF 知识库，以及在 APP 中配置并测试通过的模型通道 |
| 修改源码或构建桌面程序                 | 下文的 Node.js、Rust、MSVC 和 WebView2 开发环境          |

Python 模块声明支持 `>=3.11,<3.13`，本机配置采用 **Python 3.12.13 x64**；原开发验收使用 3.12.14。不要用默认的 Python 3.14 创建此虚拟环境。PaddlePaddle 固定为 3.0.0，其他依赖以 `rag-service/requirements.lock.txt` 为准。

手工创建的开发 venv 不包含安装包的 UTF-8 进程清单修复；Paddle 原生模型在中文开发路径下可能读取失败。开发目录建议使用英文路径，或使用完整构建脚本生成的便携运行时。安装版已修复并实测中文路径。

### 1. 创建项目独立环境

以下命令在项目根目录执行，无需激活虚拟环境。每条命令成功后再继续下一条：

```powershell
# 在下载并解压的项目根目录执行
```

已有 Windows Python Launcher 和 Python 3.12 时：

```powershell
py -3.12 -m venv .\rag-service\.venv
```

使用 Miniconda / Anaconda、没有 `py` 命令时，可以创建专用的 Python 3.12 环境，再由它创建项目虚拟环境：

```powershell
conda create -n tellwhy-pdf-python python=3.12 -y
conda run -n tellwhy-pdf-python python -m venv .\rag-service\.venv
```

以上是替代方案，不要依次重复创建。请保留用于创建 venv 的基础 Python；虚拟环境不能直接复制到另一台电脑。当前机器的历史配置见 [环境记录](docs/pdf-environment-2026-09-16.md)，完整安装包使用独立的 Python 3.12.14。

### 2. 安装锁定依赖

```powershell
.\rag-service\.venv\Scripts\python.exe --version
.\rag-service\.venv\Scripts\python.exe -m pip install -r .\rag-service\requirements.lock.txt
.\rag-service\.venv\Scripts\python.exe -m pip install --no-deps --no-build-isolation -e .\rag-service
.\rag-service\.venv\Scripts\python.exe -m pip check
```

最后一条应输出 `No broken requirements found.`。使用虚拟环境中的完整 Python 路径，避免把依赖装进 Conda base 或其他项目。首次安装需要联网。

### 3. 下载并校验本地模型

```powershell
.\rag-service\.venv\Scripts\python.exe -m tellwhy_kb models prepare --data-root .\data
.\rag-service\.venv\Scripts\python.exe -m tellwhy_kb models verify --data-root .\data
.\rag-service\.venv\Scripts\python.exe -m tellwhy_kb.indexing.reranker prepare --models-root .\data\models
.\rag-service\.venv\Scripts\python.exe -m tellwhy_kb.indexing.reranker verify --models-root .\data\models
```

首次需要下载 OCR 检测、识别、版面分析、Embedding 和 Reranker 模型；Reranker 约 1.1 GB。模型固定到项目记录的 revision，并保留逐文件 SHA-256 校验清单。下载完成后的 PDF 处理和检索在本地运行，不需要模型 API Key。AI 解释和生成是另行配置的在线功能。

下载未完成时不要把“Python 已安装”当成“可以导入 PDF”。修复模型文件后可以重新执行对应的 `prepare` 与 `verify` 命令。

### 4. 回到 APP 验证

1. 重新进入“PDF 知识库”；如果 APP 仍保留旧错误，完全退出后重新启动。
2. 点击“选择 PDF”，确认能够显示所选文件的页数和正文起始页。
3. 首次测试先选少量页面的 PDF，点击“开始导入”，等待处理结束后查看章节、检索和原文。
4. “随机学习”和“复习 Agent”需要先有已完成的知识库；调用 AI 前还需配置模型通道。

源代码仓库不包含已发布的教材知识库。根目录有 PDF 文件不代表它已经导入；只有对应的 `data/knowledge-bases/<资料>/active.json` 和完整版本产物存在时，已有资料才会显示。不要把历史验收中“已导入《计算机组成原理》”理解为每台机器都会自动带有该知识库。

### 路径、迁移与排错

| 提示或情况                       | 检查与处理                                                                                                                            |
| -------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------- |
| “本机 PDF 处理组件未就绪”        | 检查 `rag-service/.venv/Scripts/python.exe` 和 `rag-service/src/tellwhy_kb/desktop.py` 是否存在；也检查是否设置了指向旧目录的环境变量 |
| “无法启动 PDF 处理组件”          | 执行上述 `python.exe --version`；如果基础 Python 被移走，使用可用的 Python 3.12 重新创建环境                                          |
| `ModuleNotFoundError` 或缺少依赖 | 用项目虚拟环境重新安装锁定依赖和本地包，再运行 `pip check`                                                                            |
| 本地模型缺失或校验失败           | 执行基础模型的 `prepare / verify`；“相关原文”错误还需检查 Reranker                                                                    |
| 知识库列表为空                   | 首次使用需导入 PDF，或核对当前数据根目录；这不等同于 Python 环境损坏                                                                  |

源码开发模式使用项目的 Python 服务和 `data/`；完整安装版从 EXE 同级的 `pdf-runtime/` 解析资源，数据写入用户目录。自定义开发环境时，可在启动 APP 的同一终端设置：

```powershell
$env:TELLWHY_RAG_SERVICE = (Resolve-Path .\rag-service).Path
$env:TELLWHY_PYTHON = (Resolve-Path .\rag-service\.venv\Scripts\python.exe).Path
$env:TELLWHY_KB_DATA = Join-Path (Get-Location).Path 'data'
$env:TELLWHY_KB_MODELS = Join-Path $env:TELLWHY_KB_DATA 'models'
npm.cmd run tauri -- dev
```

环境变量必须由启动进程继承，在其他终端设置不会改变已经运行的 APP。安装版通常不应设置这些覆盖变量；旧的覆盖值可能使程序指向失效目录。

完整包包含 Python 和模型，个人 PDF 数据始终由用户导入。源码 ZIP 不包含模型二进制。详细命令见 [Python PDF 模块](rag-service/README.md)，界面操作见 [PDF 桌面指南](docs/pdf-desktop-guide.md)。

## 开发环境

### 环境要求

- Windows 10 1903 或更高版本，或 Windows 11。
- Node.js 22.12+ 或 24 LTS（本次使用 24.19.0）。
- npm 10 或更高版本。
- Rust stable，MSVC toolchain。
- Microsoft Visual Studio 2022 Build Tools。
- WebView2 Runtime。

安装 Visual Studio Build Tools 时需要勾选“使用 C++ 的桌面开发”。Build Tools 主体可以安装在 D 盘；Windows SDK 和部分共享组件仍可能由微软安装器放在系统盘，项目不依赖固定安装路径。

如果 PowerShell 阻止执行 `npm.ps1`，直接使用下文的 `npm.cmd`，不需要修改系统执行策略。

### 安装依赖

在项目根目录打开 PowerShell：

```powershell
npm.cmd ci
```

首次构建 Rust 部分时，Cargo 会下载并编译依赖。

### 启动桌面开发模式

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\start-source.ps1
```

此入口编译当前源码，首次安装 npm 依赖。PDF 开发环境按上一节准备；也可手动运行 `npm.cmd run tauri -- dev`。

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

1. 打开“模型设置”；也可以在知识主界面点击当前模型旁的“去配置”或“去测试”。
2. 选择 DeepSeek 或 Kimi，以及应用提供的区域和模型。
3. 输入 API Key 并保存。
4. 等待输入框清空，确认界面只显示 Key 的最后四位。
5. 点击连接测试。
6. 返回知识主界面，选择首选模型后开始生成。

不要将真实 API Key 写入 `.env`、命令行、SQLite、普通配置文件、日志、测试代码或聊天消息。

## 当前生成与去重逻辑

生成请求只向模型提供领域、生成数量、字段格式和安全边界，不会把完整历史题目列表发送给供应商。继续追问会发送当前卡片内容、当前问题及最近 6 条对话记录；选词查询还会发送用户确认的文字。本地浏览卡片本身不会发起模型请求。

模型返回后，本地会执行：

1. JSON 结构和字段范围校验。
2. 高风险主题过滤。
3. 与数据库已有问题及本批已接收问题比较。
4. 跳过被判定为重复的知识卡。
5. 保存通过校验的内容，并标记为未经核验的 AI 内容。

当前重复检测以规范化题面的连续双字符集合为基础，相似度达到 88% 才会判定重复。它可以阻止完全相同或几乎相同的问题，但仍可能漏过语义相同、措辞不同的问题，例如“IPv4 和 IPv6 有什么区别”与“IPv4 和 IPv6 的主要差异是什么”。

这是当前 MVP 的已知限制：应用尚未实现语义向量去重或稳定的“知识主题键”。从未交互的 AI 缓存卡被淘汰，或者用户彻底删除某张 AI 卡后，该问题也可能在未来再次生成。

## 数据与隐私

默认 SQLite 数据库位置：

```text
%APPDATA%\com.tellyouwhy.desktop\tell-you-why.db
```

API Key 保存在 Windows Credential Manager。SQLite 只保存供应商配置、凭据引用和最后四位掩码，不保存完整 Key。

- 下列操作只有在应用内再次确认后才会执行：不感兴趣、已狠狠涨知识、最近浏览单条删除或清空、删除自定义兴趣、清除阅读记录或偏好或全部本地数据，以及 API Key 删除或覆盖。取消收藏仍保持直接、可逆。
- 确认框默认聚焦“取消”，支持按 `Esc` 关闭；执行失败时对话框会保留错误信息并允许重试，提交期间会阻止重复操作。
- “不感兴趣”和“已狠狠涨知识”会把卡片永久移出主推荐流；后者还会取消收藏，但卡片仍保留在最近浏览中。
- 删除自定义兴趣只会先把它从当前设置草稿中移除，仍需点击“保存兴趣设置”才会持久化。
- “清除阅读记录”删除阅读、揭晓和展开交互，并清空当前会话的前进/后退栈；收藏、卡片和模型配置不受影响。
- “清除阅读记录”不会删除知识卡的追问线程；追问问题、AI 回复及实际作答模型信息会随对应知识卡保留，只有彻底删除该知识卡或清除全部本地数据时才会一并删除。
- “清除偏好”清零兴趣权重并删除自定义兴趣；内置兴趣的选择、启用状态和排序，以及历史、收藏和模型配置均保留。
- “清除全部本地数据”删除历史、收藏、偏好、应用设置、生成或导入的卡片和 API Key，保留内置演示卡并返回首次使用状态；同时将开机启动、快捷键和窗口置顶恢复为默认值。该操作无法撤销。
- 清空全部数据时会独占本地持久化通道，避免模型请求或其他写入与重置并发；重载应用数据也不会重新写入已清空的阅读记录。
- 覆盖 API Key 会单独提示旧 Key 不可恢复；删除 Key 还会移除该通道保存的模型选择和连接验证状态。新旧系统凭据采用可恢复的清理流程，未完成的凭据删除会在后续启动时继续尝试。

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

PDF Python 回归可单独运行。使用新的项目内临时目录，避免与旧的系统 pytest 目录权限冲突：

```powershell
New-Item -ItemType Directory -Force -Path .\tmp | Out-Null
$testTemp = Join-Path (Get-Location).Path ('tmp\pytest-pdf-' + [Guid]::NewGuid().ToString('N'))
.\rag-service\.venv\Scripts\python.exe -m pytest .\rag-service\tests -q --basetemp $testTemp
```

2026-09-16 本机 PDF 环境复测：69 项 Python 测试通过；100 项锁定依赖、模型文件校验、PDF 读取渲染、真实 OCR / Embedding / Reranker 和 desktop 协议检查通过。OCR 样本仍保留 `needs_review` 质量状态，详见 [本次环境验收](docs/pdf-environment-2026-09-16.md)。

既有功能回归记录（本次没有重跑前端 / Rust）：

- 前端测试 132 项通过（19 个测试文件）。
- Rust 常规测试 164 项通过，包含真实 Python 进程与取消集成测试；Python RAG 69 项通过。评测脚本此前 27 项回归通过。
- 9 项实机/真实模型/显式评测入口默认忽略；另行运行复习 Agent 受控评测，10 个场景通过。Harness 故障注入 8 个场景各重复 3 次，24/24 次通过。
- ESLint、TypeScript、Vite 生产构建和严格 Clippy 通过；修改文件的 Prettier 检查通过。Vite 提示主脚本略超 500 kB，构建成功。
- 本次 Harness 验收使用受控模型与独立临时数据库，没有真实模型 API 请求。此前真实教材与模型实测见 [教材评测记录](docs/textbook-agent-roadmap.md)，不与本次流程通过率混算。

## 构建 Windows 完整安装包

先安装上述开发工具。在项目根目录执行：

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\build-windows.ps1
```

脚本会下载并校验固定版本的独立 Python，按锁文件安装依赖，准备和校验模型，附带 MSVC 本地运行库、评测脚本及第三方许可，再编译 Tauri 程序并使用 Inno Setup 7 x64 生成安装包。首次需要联网并下载较多文件；本机已校验模型会复用。构建记录与实现决定见 [Windows 分发记录](docs/windows-distribution.md)。

产物：

- `release-artifacts/Tell-You-Why_0.1.1_windows-x64-full-setup.exe`
- `release-artifacts/SHA256SUMS.txt`
- `packaging/release.json`：一键启动使用的版本、文件大小和 SHA256。重新构建发布时与对应安装包一起更新。

安装脚本位于 `packaging/windows.iss`；构建工具及 WebView2 下载的 SHA256 固定在 `packaging/windows-tools.json`。只执行 `npm.cmd run tauri -- build` 不包含 PDF 完整资源；分发请使用本节命令。不要将 `.venv`、`node_modules`、模型或安装包提交进 Git；安装包上传 GitHub Release。

安装后的只读诊断（输出 JSON，不启动 GUI）：

```powershell
Start-Process "$env:LOCALAPPDATA\Programs\Tell You Why\tell-you-why.exe" -ArgumentList '--check-pdf', ('"' + "$env:TEMP\tellwhy-pdf-check.json" + '"') -Wait
Get-Content "$env:TEMP\tellwhy-pdf-check.json" -Encoding UTF8
```

此诊断验证 Rust 能找到安装资源、调用 Python 并读取模型就绪状态。真实 OCR / 向量 / 重排自检脚本位于安装目录 `pdf-runtime/check_runtime.py`；验收结果见分发记录。

## 项目结构

```text
src/                         React 前端、页面、组件和前端测试
src-tauri/src/               Rust 命令、SQLite、桌面能力和模型适配器
src-tauri/resources/         随应用提供的演示知识卡
src-tauri/capabilities/      Tauri 权限配置
rag-service/                 Python PDF / OCR / 检索模块与独立环境
data/models/                 构建时复用的本地模型（不随源码提交）
data/knowledge-bases/        本机导入的 PDF 知识库和版本化产物
docs/                        实施计划、技术决策、安全与人工验收文档
examples/                    JSON/CSV 内容导入模板
packaging/                   固定版本、校验信息和打包辅助脚本
scripts/                     安装启动、源码启动、完整构建脚本
start.cmd                    一键下载、校验、安装并启动
启动 Tell You Why.cmd        同一入口的中文别名
```

## 仍需人工完成

- 准备并复核 100–300 条正式知识卡内容。
- 使用低额度 DeepSeek 和 Kimi 真实账户分别测试连接、单批生成、批量生成和自动故障切换。
- 在 420 × 560 与 360 × 440 真实窗口中复核布局和键盘操作。
- 验收系统托盘、快捷键冲突、通知、开机启动、窗口恢复和多显示器拔插。
- 补充干净 Windows 虚拟机、跨版本升级和卸载保留数据的验证。
- 配置 Windows 代码签名证书并进行安全软件抽检。

完整项目状态请查看 [MVP 实施计划](docs/implementation-plan.md) 和 [Windows 人工测试清单](docs/manual-test-checklist.md)。

## 项目文档

- [原始 MVP 设计方案](Tell%20You%20Why%20-%20MVP设计方案.md)
- [MVP 实施计划与 P0 追踪](docs/implementation-plan.md)
- [技术决策记录](docs/decisions.md)
- [内容导入与审核流程](docs/content-import.md)
- [复习 Agent 使用与持久化](docs/review-agent-guide.md)
- [Harness v1 使用与架构](docs/harness-v1-guide.md)
- [Harness 分阶段开发与验收](docs/harness-v1-development.md)
- [Harness v2 工具执行与进程取消](docs/harness-v2-guide.md)
- [Harness v2 前两阶段验收](docs/harness-v2-development.md)
- [真实教材评测与持续复习验收](docs/textbook-agent-roadmap.md)
- [安全复核](docs/security-review.md)
- [Windows 人工测试清单](docs/manual-test-checklist.md)

原始设计方案是产品范围、功能要求和验收标准的主要依据，开发过程中保持原样。
