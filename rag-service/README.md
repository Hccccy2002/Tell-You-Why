# Python PDF 知识库模块

本模块完成 `PDF → 提取/OCR → 章节 → 分块 → 关键词与向量索引 → 本地知识库`，并通过 `desktop.evidence` 为 RAG 提供带版本和原文位置的证据包。已支持通过桌面 APP 的 **PDF 知识库** 页面点击操作，CLI 继续保留。Tauri 调用相同的知识库流程，生成模型和业务持久化由 Rust 负责。详见 [PDF 界面](../docs/pdf-desktop-guide.md)及[教材问答与学习卡](../docs/rag-minimal-loop-guide.md)。随机学习调度留到下一阶段。

一份 PDF 对应一次知识库构建。相同文件、配置和运行时复用任务；更换文件或配置生成新任务，已发布版本保留。当前不把多份 PDF 自动合并成一个索引。

## 已验证环境

- Windows x64、CPU 推理。原开发验收为 Python 3.12.14；2026-09-16 本机独立环境采用 Python 3.12.13。模块要求 Python >=3.11,<3.13，不能使用默认的 Python 3.14。
- PaddleOCR 3.7.0、PaddleX 3.7.2、PaddlePaddle **3.0.0**。
- Sentence Transformers 6.0.1、PyTorch 2.14.0。
- pypdf / PDFium、SQLite FTS5 / jieba、NumPy。
- 精确依赖见 `requirements.lock.txt`。PaddlePaddle 的版本固定源于实际 Windows 推理兼容性测试。

模型名称和 revision 固定在 `src/tellwhy_kb/models.py`。下载后的文件逐个记录 SHA-256；OCR 使用 `PP-OCRv5_server_det`、`PP-OCRv5_server_rec` 和 `PP-DocLayout-S`，Embedding 使用 `BAAI/bge-small-zh-v1.5`，输出 512 维向量。

## 安装与模型准备

以下命令均在项目根目录 `D:\Tell-You-Why` 执行。完整的 Python / Conda 安装分支、依赖检查和错误排查见 [根目录 README](../README.md#pdf-知识库运行环境)。已有环境先执行 `python.exe --version` 和 `python.exe -m pip check`；依赖安装完成后仍需准备并验证模型。

```powershell
py -3.12 -m venv rag-service\.venv
.\rag-service\.venv\Scripts\python.exe -m pip install -r rag-service\requirements.lock.txt
.\rag-service\.venv\Scripts\python.exe -m pip install --no-deps --no-build-isolation -e rag-service
```

首次准备模型需要联网；后续处理与检索使用本地模型，不需要 API Key。

```powershell
.\rag-service\.venv\Scripts\python.exe -m tellwhy_kb models prepare --data-root data
.\rag-service\.venv\Scripts\python.exe -m tellwhy_kb models verify --data-root data
```

“相关原文”的 Top 5 重排还需要准备中文 / 英文交叉编码器（约 1.1 GB，首次下载需联网）：

```powershell
.\rag-service\.venv\Scripts\python.exe -m tellwhy_kb.indexing.reranker prepare --models-root data/models
.\rag-service\.venv\Scripts\python.exe -m tellwhy_kb.indexing.reranker verify --models-root data/models
```

该模型固定为 `BAAI/bge-reranker-base` 的 `2cfc18c9415c912f9d8155881c133215df768a70` 版本，保存在 `data/models/reranker/`，使用独立的 SHA-256 清单。无需重建已有 PDF 索引；重排仅在本地运行，不调用 LLM API。模型缺失或校验失败时，“相关原文”会显示可重试的错误，生成卡片、AI 解释及学习记录仍按原有流程工作。

Top 5 使用生成后的题目查询：BM25 + BGE 向量混合召回 24 个候选分块，取来源正文及同节可用相邻段落，按完整段落去重，再通过交叉编码器逐条评分、降序选出最多 5 条。长段落分窗口评分并保留完整展示文字和 PDF 页码。检索仍遵守教材、版本和章节范围；不靠补造摘录凑满数量。原有生成证据包及引用快照保留用于核对，不被展示结果覆盖。实现与评测见 [Top 5 原文检索](../docs/related-sources-top5.md)。

## 按阶段执行教材案例

```powershell
# 1. 查看页数、哈希、书签和已有文本层的来源
.\rag-service\.venv\Scripts\python.exe -m tellwhy_kb inspect --pdf "计算机组成原理.pdf"

# 2. 先做代表页，结果保留在 work 下，不发布为整本知识库
.\rag-service\.venv\Scripts\python.exe -m tellwhy_kb ingest --pdf "计算机组成原理.pdf" --kb computer-organization --config rag-service\configs\ingest.yaml --pages "3,9,50,100,300" --until structure --overrides rag-service\configs\computer-organization-overrides.json

# 3. 全量处理并发布；复用配置一致的已有页结果
.\rag-service\.venv\Scripts\python.exe -m tellwhy_kb ingest --pdf "计算机组成原理.pdf" --kb computer-organization --config rag-service\configs\ingest.yaml --overrides rag-service\configs\computer-organization-overrides.json

# 4. 独立校验发布产物
.\rag-service\.venv\Scripts\python.exe -m tellwhy_kb validate --kb computer-organization

# 5. 返回教材片段、章节和来源位置
.\rag-service\.venv\Scripts\python.exe -m tellwhy_kb search --kb computer-organization --query "CPU包括哪两大部分？" --top-k 5
```

案例配置采用 220 DPI、重新 OCR、第 9 页开始作为正文候选。前 8 页仍保存识别结果，默认不参与学习检索。章节修正文件绑定本书 SHA-256，将错误的 5.5 书签定位到 PDF 第 198 页，并补上第 234 页的 6.2 小节。

处理其他 PDF 时使用新的 `--kb`，省略案例配置和修正文件，或创建自己的配置；默认从第 1 页开始。`--data-root` 选择数据目录，`--models-root` 可复用另一目录的模型。`--workers` 默认 1，允许 1–4；CPU 数量、页面复杂度和原生库线程池都会影响并行收益。

## 恢复、取消与阶段产物

进度事件和命令结果以 JSON 输出。任务 ID 出现在 `extraction_started` 事件中，也对应 `work` 下的目录名。

```powershell
.\rag-service\.venv\Scripts\python.exe -m tellwhy_kb status --job <job_id> --kb computer-organization
.\rag-service\.venv\Scripts\python.exe -m tellwhy_kb cancel --job <job_id> --kb computer-organization
.\rag-service\.venv\Scripts\python.exe -m tellwhy_kb resume --job <job_id> --kb computer-organization --overrides rag-service\configs\computer-organization-overrides.json
```

取消请求让正在识别的页完成后停止。Ctrl+C 保留已提交检查点；恢复时重新处理未提交或损坏的页。每个任务只允许一个写入者。页范围可从样本扩大到全书；`--until extract`、`structure`、`chunks` 可在指定阶段停止。

模型或推理运行时发生变化时，使用 `ingest` 建立新任务。恢复时继续传入原有的 `--overrides`、`--terms` 等整理参数。正文分块以 320 Token 为目标、40 Token 重叠，包含章节前缀的实际模型输入必须不超过 512 Token。

```text
data/
  models/
    det/ rec/ layout/ embedding/
    model-manifest.json
  knowledge-bases/<kb>/
    sources/<sha256>.pdf
    work/<job_id>/
      job.json, job.sqlite, progress.json, stage.json
      pages/*.json, crops/*.png
      chapters.json, blocks.jsonl, chunks.jsonl
    versions/<version>/
      knowledge.sqlite, embeddings.npy, terms.txt
      assets/*.png, manifest.json
    reports/
    active.json
```

`active.json` 只指向通过校验的不可变版本。失败的发布不会替换旧指针；中断时遗留的 `.building` 目录不参与读取。发布版本中的正文、页数据、章节、原始文字和来源映射均在 SQLite 内，图片在 `assets` 内；读取不依赖 `work`。备份时保存整个知识库目录及匹配的 Embedding 模型。

## 质量和检索约定

- `ready` 表示流水线结构校验通过且没有待检查页；`partial_ready` 表示可检索的内容已发布，同时有未可靠解析的区域。两者都不等于全书已人工校对。
- 页状态包括 `success`、`needs_review`、`blank`、`excluded`、`failed`；尚未处理的页为 `pending`。默认不发布存在未完成/失败页的任务，只有显式 `--allow-partial` 才允许这种发布。
- `needs_review` 表示该页存在受限区域；同页通过规则检查的正文仍可入库，因此页面状态和可用正文覆盖率分别统计。
- 公式、图表、未匹配版面的文字、低置信度文字、可疑数学符号、`I/O` 与 `I/0` 歧义等保留原文/原图并排除出默认学习证据。不自动猜测或改写这些内容。
- 物理页码从 1 开始；书内页码单独保存，不假设全书固定偏移。坐标单位是旋转后的 PDF 显示页点数，原点在左上角。
- `chunk_locations` 保存块 ID、规范化字符区间、原始字符区间、页码和坐标。跨页片段有多个位置记录。
- `--mode keyword` 使用分词后的 FTS5/BM25；`dense` 使用归一化向量的余弦相似度；`hybrid` 使用加权 RRF，关键词/语义权重为 1:3、排名常数为 60。此设置在开发问题集上选择，完整参数随检索结果返回。`--chapter` 接受章/节 ID 或完整标题，筛选范围包含其下级节点，并在排序取前几条之前应用。
- 检索返回 `answerability: not_assessed`。排名分数不是事实正确率，也不是资料足以回答问题的证明；本阶段不生成答案。

## 自测与案例验收

```powershell
.\rag-service\.venv\Scripts\python.exe -m pytest rag-service\tests -q
.\rag-service\.venv\Scripts\python.exe -m ruff check rag-service\src rag-service\tests rag-service\scripts
.\rag-service\.venv\Scripts\python.exe rag-service\scripts\evaluate_case.py --kb-root data\knowledge-bases\computer-organization --models-root data\models
```

单元/集成测试覆盖 PDF 旋转与坐标、恢复和损坏缓存、章节边界、字符来源映射、分块覆盖、索引一致性和原子发布。小型测试不下载模型。

案例验收读取固定的 20 段视觉转录正文及 30 条检索探针。正文字符错误率在去除空白和 Unicode 标点后计算，保留字母、数字和数学符号；它不评估其他位置的漏识别、标点准确率或公式结构。报告同时给出全部有源问题的命中率、已有合格证据的问题命中率和被排除内容造成的覆盖缺口。6 条不支持的问题检查检索接口没有声称已回答；回答质量和拒答正确率留给后续 RAG 阶段。

上述证据可用性和 Hit@5 按已标注的物理页码与原句计算；其他页面的等价证据尚未穷尽标注，标注证据未入库并不证明全书都无法回答该问题。

本机教材的完整运行结果见 `data/knowledge-bases/computer-organization/reports`；本次交付状态见 `docs/pdf-preparation-acceptance.md`。
