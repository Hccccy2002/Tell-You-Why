# Windows 完整包 0.1.1

2026-09-16，目标为 Windows 10 1903+ / Windows 11 x64。仓库按用户选择保持私有；下载使用有权限的 GitHub 账号。

## 分发内容

- Inno Setup 7 EXE 完整安装程序：Python 3.12.14、项目锁定依赖、五组固定修订的本地模型、MSVC app-local DLL、WebView2 离线安装组件。
- `start.cmd` / 中文别名：下载固定 Release、核对 SHA256、当前用户安装并启动。支持 Git Credential Manager、GitHub CLI 和用户手动下载到源码根目录的同名安装包。
- `scripts/start-source.ps1`：编译运行当前源码。
- `scripts/build-windows.ps1`：准备资源、构建安装包、生成 SHA256 和启动清单。
- `--check-pdf <output.json>`：通过实际 Rust 适配器读取 PDF 组件状态，供安装诊断使用。
- `pdf-runtime/check_runtime.py --out <directory>`：模型文件校验和真实离线 PDF / OCR / Embedding / Reranker 推理。

Git 中只保存代码、文档、版本与校验清单。安装包上传 Release。个人 PDF、知识库、API Key、账号凭据均不进入安装包。Python 依赖随原始许可证分发，模型卡、来源修订和许可位于 `pdf-runtime/third-party/`。

## 运行路径

| 内容                            | 完整安装版                                         |
| ------------------------------- | -------------------------------------------------- |
| Python / 服务 / 模型 / 评测脚本 | EXE 同级 `pdf-runtime/`                            |
| PDF 数据及模型运行缓存          | `%LOCALAPPDATA%/com.tellyouwhy.desktop/pdf-data/`  |
| 主数据库                        | `%APPDATA%/com.tellyouwhy.desktop/tell-you-why.db` |
| 一键启动默认安装位置            | `%LOCALAPPDATA%/Programs/Tell You Why/`            |
| 下载缓存                        | `%LOCALAPPDATA%/TellYouWhy-Installer/<version>/`   |

Debug 模式保留项目 .venv/data 布局，Release 不回退到构建机路径。显式环境变量覆盖由启动进程继承，通常无须设置。代码与模型只读取；缓存和用户资料写入用户目录。默认不迁移开发目录中的旧知识库。

## 环境与检查

- Windows 11 10.0.26200 x64。
- Node 24.19.0 / npm 11.17.0 / Rust 1.98.0 / MSVC Build Tools。
- 独立 CPython 3.12.14，来源及 SHA256 固定在 `packaging/python-runtime.json`。
- 前端 132/132 测试通过，生产构建通过。
- Rust 167 项通过、9 项显式集成/真实模型测试忽略；新增测试覆盖 Release 路径、中文空格路径与 Debug 回退。
- Python 原有 69 项通过；新增 1 项缓存隔离测试通过。
- 严格 Clippy、ESLint 通过。最终构建和安装检查结果见下文。

## 已知限制

- 本次不是全新 Windows 虚拟机验收。宿主机已安装 WebView2，缺失 WebView2 时的安装分支尚需干净机器验证。
- 未配置代码签名，可能出现 Windows 未知发布者提示。
- 内置知识卡为演示数据，不等同于正式审核内容。真实模型付费调用未在本次分发验证中执行。
- PDF 的 OCR 质量仍取决于资料；推理组件可运行不代表所有 PDF 都能无误识别。
- 没有整体项目开源许可证变更；仓库保持私有。第三方组件继续适用各自许可证。

## 失败样本与修正

最初使用 Tauri 的 NSIS 完整打包。编译程序成功，但 NSIS 压缩约 2.7 GB 资源时失败：`Internal compiler error #12345: error mmapping datablock to 2735420`。此构建没有产生可交付安装包，未标为通过。

最终改为官方 Inno Setup 7.1.0 x64 编译器；Windows 安装脚本单独版本化，直接打包同一 Release EXE 与经过校验的运行时。保留原始 NSIS 失败日志摘要。安装器工具及微软 WebView2 离线组件先校验固定 SHA256 和发布者签名。

## PDF 处理实测

- 独立 Python 在仅包含 Windows 系统目录的 PATH 下完成模型哈希校验、PDF 渲染、OCR、2 × 512 向量编码和 Reranker 推理；不使用 Conda / 全局 Python / 模型 API。
- 实际桌面适配器流程：合成 PDF → prepare → run_import → 发布版本 → CPU 混合检索 → 重复选择同一文件。发布 1 页、2 个 chunk，检索有结果，重复导入复用旧版本。
- 导入状态为 **partial_ready**：7 个块中 3 个可索引，其余内容被质量过滤。此样本证明组件与发布/检索链路可运行，不表示 OCR 全文质量验收通过。完整结果保留在验收 JSON 中。
- Windows PowerShell 5.1 下，损坏安装包被 SHA256 检查拒绝，没有创建安装目录或执行文件。

### 中文安装目录的原生库问题

首次实际安装到包含中文和空格的目录后，Rust catalog 和 PDF 渲染通过，但 Paddle 3.0.0 读取 `models/det/inference.json` 失败。文件存在，Windows 短路径也没有可用的 ASCII 替代名；宿主默认代码页为 936。此样本按失败保留，最初草稿安装包不发布。

修正为构建时保留 Python 原有清单内容，给随包的 `python.exe` 与 `pythonw.exe` 添加 `activeCodePage=UTF-8`。运行时进程代码页变为 65001，同一中文路径下真实 OCR、Embedding 和 Reranker 均通过。该修改不改变系统区域设置。构建脚本自动执行清单修改与代码页断言，并重新生成安装包。

最低系统要求相应设为 Windows 10 1903，详见 [Microsoft 进程级 UTF-8 文档](https://learn.microsoft.com/en-us/windows/apps/design/globalizing/use-utf8-code-page)。第三方说明标记了对 Python EXE 清单的这项修改。原始失败与修复结果保存在 `docs/validation/windows-release-0.1.1/`。

## 最终文件

- 文件：`Tell-You-Why_0.1.1_windows-x64-full-setup.exe`
- 大小：1,754,396,463 字节（约 1.75 GB）
- SHA256：`3752601b786aa59955f528441baa88a71059fdc233cb8c8937697db090c83cd1`
- 完整性清单：`packaging/release.json`，Release 同时提供 `SHA256SUMS.txt`。

## 最终安装包验收（2026-09-16）

最终 SHA256 对应的安装包通过 Windows PowerShell 5.1 一键脚本安装到全新目录 `tmp/完整安装 复测/Tell You Why`，安装退出码为 0。安装后的检查全部使用该目录内的 EXE、Python 与模型：

| 检查                           | 结果                                                     | 证据                                   |
| ------------------------------ | -------------------------------------------------------- | -------------------------------------- |
| 实际 Rust PDF 适配器           | 通过；组件与模型就绪，首次资料列表为空                   | `final-installed-adapter.json`         |
| Python 进程代码页              | 通过；65001，中文和空格路径可加载模型                    | `final-installed-runtime.json`         |
| PDF 渲染、OCR、向量与重排      | 通过；向量尺寸 2 × 512，相关文本重排得分高于无关文本     | `final-installed-runtime.json`         |
| 实际导入、发布、检索、重复导入 | 链路通过；1 页、2 个 chunk、有检索结果，重复导入复用版本 | `final-installed-import.json`          |
| 导入内容完整性                 | 部分就绪；需要人工复核，不能标为全文质量通过             | 同上，`partial_ready` / `needs_review` |

上述证据位于 `docs/validation/windows-release-0.1.1/`。没有把开发虚拟环境复制为安装运行时；没有修改 Windows 系统区域设置。
