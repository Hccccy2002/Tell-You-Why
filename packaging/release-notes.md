Windows x64 完整安装包，包含独立 Python 3.12.14、PDF/OCR 依赖、OCR / Embedding / Reranker 本地模型及 WebView2 离线安装组件。

安装方法：

1. 下载 `Tell-You-Why_0.1.1_windows-x64-full-setup.exe`，双击安装。
2. 打开 APP 的“PDF 知识库”，选择自己的 PDF 并开始导入。
3. 如下载源码，可运行根目录 `start.cmd` 自动下载、校验、安装并启动；私有仓库需已有 Git / GitHub CLI 登录，或把同名 EXE 放在源码根目录。

源码开发入口：`powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\start-source.ps1`。
完整构建入口：`powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\build-windows.ps1`。

安装包未签名，请核对同页 SHA256SUMS.txt。建议预留 10 GB 磁盘空间。安装包不含个人 PDF、知识库或 API Key；本地 PDF 处理可离线运行，AI 功能仍需用户自行配置模型通道。

这是 MVP 预发布版；内置知识卡为演示内容。实测范围和限制见源码中的 `docs/windows-distribution.md`。
