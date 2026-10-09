# PDF 运行环境验证摘要（2026-09-16）

本页保留历史验证结果和复现入口。包含本机路径、用户名、完整错误堆栈和截图的原始运行附件不随当前源码分发；以下记录不是本次仓库整理重新运行的结果。

## 环境与处理

当时 PDF 页面提示本地组件未就绪，原因是项目 Python 虚拟环境缺失，而系统默认 Python 超出了模块要求的 `>=3.11,<3.13` 范围。

使用 Python 3.12.13 x64 创建项目独立 venv，安装锁定依赖与本地模块，并准备固定 revision 的 OCR 检测、识别、版面分析、Embedding 和 Reranker 模型。模块与模型的现行准备方式见 [README](../README.md#pdf-知识库运行环境)。

## 历史结果

| 检查         | 结果                                              |
| ------------ | ------------------------------------------------- |
| 依赖         | 100 项锁定版本一致，pip check 无冲突              |
| 模型         | OCR、版面分析、Embedding 与 Reranker 文件校验通过 |
| Python 回归  | 69 通过，0 失败；使用独立项目临时目录             |
| PDF 渲染     | 合成 PDF 可读取，页面可渲染                       |
| OCR          | 本地推理识别已知测试文字，输出仍标记为需要复核    |
| Embedding    | 两段输入产生 2 × 512 维有限值向量，归一化检查通过 |
| Reranker     | 相关短文本得分高于无关短文本                      |
| desktop 协议 | 模型就绪，PDF 页数读取通过                        |
| 应用界面     | 组件缺失提示消失，文件选择器可打开并取消          |

## 失败与验证边界

首次 pytest 运行因已有系统临时目录拒绝访问，出现 25 通过、44 个夹具初始化错误。改用项目 `tmp/` 中的新目录后，69 项通过。

文件选择器检查没有完成“通过 UI 选择并导入”的整个流程。OCR 样本仍是 `needs_review`；组件能运行不证明任意教材全文识别质量，两个短文本的重排也不代替检索质量评测。

本轮没有调用在线生成模型，没有重跑前端或 Rust 全套测试，没有创建新安装包。基础 Python 是 venv 的运行依赖，虚拟环境不能直接复制到另一台机器。

## 复现

在项目根目录按 README 准备环境后执行：

```powershell
.\rag-service\.venv\Scripts\python.exe --version
.\rag-service\.venv\Scripts\python.exe -m pip check
.\rag-service\.venv\Scripts\python.exe -m tellwhy_kb models verify --data-root .\data
.\rag-service\.venv\Scripts\python.exe -m tellwhy_kb.indexing.reranker verify --models-root .\data\models

New-Item -ItemType Directory -Force -Path .\tmp | Out-Null
$pdfTestTemp = Join-Path (Get-Location).Path ('tmp\pytest-pdf-' + [Guid]::NewGuid().ToString('N'))
.\rag-service\.venv\Scripts\python.exe -m pytest .\rag-service\tests -q --basetemp $pdfTestTemp
```
