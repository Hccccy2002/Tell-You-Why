# PDF 知识库：桌面使用与开发说明

本次构建与测试结果见 [桌面接入验收记录](pdf-desktop-acceptance.md)。

## 使用方法

1. 双击项目根目录的 `启动 Tell You Why.cmd`，打开更新后的 APP。
2. 点击右上角 **PDF**，或打开菜单选择 **PDF 知识库**。
3. 点击 **选择 PDF**，在系统文件选择框中选中资料。
4. 核对文件名、页数和正文起始页，再点击 **开始导入**。起始页采用 PDF 实际页序号；例如案例教材的正文从第 9 页开始。
5. 在资料详情中查看处理进度。扫描教材使用本地 OCR，耗时取决于页数、版式和机器性能；同一时刻处理一本资料。
6. 点击 **暂停处理** 会保存当前页面后停止排入后续页面。若已经开始建立索引，将完成本次索引。点击 **继续处理** 复用已完成且校验通过的页面。
7. 处理完成后，可切换 **章节 / 检索 / 原文**。检索支持语义与关键词混合，也支持仅关键词。点击片段下方的页码可核对原始扫描页。

切换 APP 页面或关闭到托盘不影响导入；导入进程也可在完全退出 APP 后继续运行。重新打开 APP 可看到进度。如果机器关机或进程意外中断，任务会显示为可继续，成功保存的页面不会丢失。需要停止处理时请先在界面点击暂停。

只有本机保留了已发布的知识库数据，资料才会显示在“我的资料”中；源代码和根目录 PDF 文件不会自动成为已导入资料。再次选择同一 PDF 会打开已有知识库，不重新 OCR，也不会覆盖已经验收的章节修正。当前界面不提供重新配置已发布资料的重建入口；需要不同处理配置时仍可使用 CLI 创建任务。

## 内容状态

- **可以浏览**：资料已建立索引。
- **部分内容可用**：已通过检查的文字可浏览和检索；部分图表、公式、缺字或不确定符号被排除。原始 PDF 页面始终可核对。
- **等待继续 / 处理失败**：可查看错误并继续处理。

页面上的可检索比例以“已经识别出的正文文字”为分母，不代表对原始 PDF 所有视觉内容的识别率。检索页展示相关原文；“问答与学习”提供 RAG 回答和单张教材学习卡，操作见[教材问答说明](rag-minimal-loop-guide.md)。教材卡片目前单独保存，随机学习调度留到下一阶段。

## 接入方式

```text
React PDF 知识库页面
  → Tauri 受限命令（kb_read / kb_import / kb_resume / kb_pause）
    → 本地 Python desktop 适配器
      → OCR / 章节 / 分块 / 索引 / 版本化发布
      → data/knowledge-bases/
```

- Rust 通过独立参数和 stdin JSON 调用 Python，不拼接 shell 命令，不开放本地 HTTP 端口。
- PDF 由已有的系统文件选择权限导入；没有添加任意文件读取或 shell 前端权限。
- 查询和页面渲染在后台线程执行；导入使用独立无控制台窗口进程，日志在任务目录的 `desktop-worker.log`。
- 页面每 3 秒读取状态。Rust 防止同一进程重复启动，Python 操作系统文件锁避免多个 APP 实例同时执行导入。
- 模型和源文件、数据库、向量索引在使用时校验。模型推理保持离线，PDF 不会上传到模型服务。
- 切换资料或章节时，已失效的异步返回不会覆盖当前显示内容。
- 使用原始 PDF 的页序号定位引用，书内页码另行显示；扫描图可放大并滚动查看。

## 当前运行环境与分发边界

0.1.1 完整安装包包含独立 Python 3.12.14、锁定依赖和五组本地模型，安装后即可选择自己的 PDF；源码开发环境仍按 [README](../README.md#pdf-知识库运行环境) 准备。

安装版从 EXE 同级 `pdf-runtime/` 查找 Python、服务和模型，导入资料与缓存保存在 `%LOCALAPPDATA%/com.tellyouwhy.desktop/pdf-data/`。源码开发版继续使用项目 `rag-service/.venv` 和 `data/`。安装版不会自动迁移开发目录的知识库。不要只复制 EXE 而丢弃资源目录。

可用环境变量覆盖 `TELLWHY_RAG_SERVICE`、`TELLWHY_PYTHON`、`TELLWHY_KB_DATA`、`TELLWHY_KB_MODELS`、`TELLWHY_EVALS`；正常安装无需设置。PDF 界面尚不提供删除资料，通用知识卡清理不删除 PDF 知识库。

## 开发验证

```powershell
npm.cmd run build
npm.cmd run lint
npm.cmd test
cargo test --offline --manifest-path src-tauri/Cargo.toml
rag-service\.venv\Scripts\python.exe -m pytest rag-service/tests -q
npm.cmd run tauri -- build --no-bundle -- --offline
```

源码开发使用 `scripts/start-source.ps1`；完整分发构建使用 `scripts/build-windows.ps1`；根目录 `start.cmd` 安装并启动已发布的固定版本。
