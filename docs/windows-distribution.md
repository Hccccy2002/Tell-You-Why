# Windows 完整包 0.1.1

本页汇总 2026-09-16 的历史分发实现与验收结果，目标为 Windows 10 1903+ / Windows 11 x64。该版本早于 main 的后续功能。机器运行 JSON、截图和完整失败日志不随当前源码分发；以下是历史记录，不是本次仓库整理的重新验收结果。

## 分发与构建

- Inno Setup 7 EXE 完整安装程序包含独立 Python 3.12.14、锁定依赖、五组本地模型、MSVC app-local DLL 和 WebView2 离线组件。
- `start.cmd` / 中文别名下载固定 Release，校验大小与 SHA-256，再按当前用户安装并启动。下载支持公开资产；需要鉴权时可以使用 Git Credential Manager 或 GitHub CLI。
- `scripts/start-source.ps1` 编译运行当前源码。
- `scripts/build-windows.ps1` 准备资源并构建完整安装包，生成哈希与发布清单。
- `--check-pdf <output.json>` 通过实际 Rust 适配器读取 PDF 组件状态。
- 随包的 `pdf-runtime/check_runtime.py --out <directory>` 检查模型并执行本地 PDF / OCR / Embedding / Reranker 推理。

源码仓库只保留构建代码、版本及校验清单；安装包放在 GitHub Releases。个人 PDF、知识库和账号凭据不进入安装包。构建脚本收集第三方许可证、模型来源与固定修订。

## 运行路径

| 内容                            | 完整安装版                                         |
| ------------------------------- | -------------------------------------------------- |
| Python / 服务 / 模型 / 评测脚本 | EXE 同级 `pdf-runtime/`                            |
| PDF 数据与运行缓存              | `%LOCALAPPDATA%/com.tellyouwhy.desktop/pdf-data/`  |
| 主数据库                        | `%APPDATA%/com.tellyouwhy.desktop/tell-you-why.db` |
| 默认安装位置                    | `%LOCALAPPDATA%/Programs/Tell You Why/`            |
| 下载缓存                        | `%LOCALAPPDATA%/TellYouWhy-Installer/<version>/`   |

Debug 模式使用项目 `.venv` / `data` 布局，Release 不回退到构建机路径。模型与程序资源只读，用户资料和缓存写入用户目录；不会自动迁移开发目录知识库。

## 发布文件

- 文件：`Tell-You-Why_0.1.1_windows-x64-full-setup.exe`
- 大小：1,754,396,463 字节（约 1.75 GB）
- SHA-256：`3752601b786aa59955f528441baa88a71059fdc233cb8c8937697db090c83cd1`
- 完整性清单：[packaging/release.json](../packaging/release.json)

构建工具、Python 运行时和模型来源固定在 `packaging/` 的清单及准备脚本中。重新发布时须让版本、安装包与哈希保持一致。

## 历史验证

| 检查           | 当时结果与范围                                                                   |
| -------------- | -------------------------------------------------------------------------------- |
| 前端           | 132 项测试通过，生产构建通过                                                     |
| Rust           | 167 项通过、9 项显式集成 / 真实模型测试默认忽略                                  |
| Python         | 原有 69 项通过，新增缓存隔离测试通过                                             |
| 静态检查       | Clippy 与 ESLint 通过                                                            |
| 独立运行时     | 仅包含 Windows 系统目录的 PATH 下，模型校验、PDF 渲染、OCR、向量编码与重排可执行 |
| 安装后适配器   | 组件及模型就绪，初始资料列表为空                                                 |
| 合成 PDF 导入  | 导入、发布、CPU 混合检索与重复导入复用通过                                       |
| 中文与空格目录 | 修正 Python 进程清单后，OCR、Embedding 与 Reranker 可执行                        |
| 损坏安装包     | SHA-256 校验拒绝，不进入安装                                                     |
| 根目录启动器   | 下载校验修正后，固定版本安装、复用已安装版本及 GUI 启动通过                      |

合成导入样本只发布 1 页、2 个 chunk，资料状态为 `partial_ready` / `needs_review`。链路通过不代表 OCR 全文质量通过。启动器复用已安装版本的耗时也不等于 GUI 启动耗时。

## 失败与修正

### 大体积资源的安装器

最初使用 Tauri NSIS 打包约 2.7 GB 资源时，出现 `error mmapping datablock`，未产生可交付安装包。随后改用 Inno Setup，复用同一 Release EXE 和经校验的运行资源。

### 中文目录与 Python 原生库

中文目录中的文件存在，但 Paddle 读取模型失败。修正时保留 Python 原有 EXE 清单，添加 `activeCodePage=UTF-8`，进程代码页变为 65001，同一路径下真实推理通过。修改只作用于随包 Python，不改变系统区域设置。

### PowerShell 5.1 下载后校验

下载成功后，原脚本无法解析 `Get-FileHash`，校验阶段停止。改用 .NET SHA-256 流式计算；仅当 `.partial` 文件大小与哈希均匹配时复用。修复后安装与重复运行通过，损坏文件仍在安装前被拒绝。

## 已知限制

- 历史检查没有覆盖全新 Windows 虚拟机；缺少 WebView2 时的安装分支仍需验证。
- 未配置代码签名，升级与更多干净系统场景待完善。
- 分发验证没有运行真实模型付费调用，也没有测量长期学习效果。
- 固定发布版不包含 main 全部后续功能，当前源码运行见 [README](../README.md#源码启动)。
