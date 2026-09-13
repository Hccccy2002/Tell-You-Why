# 相关原文：按题目选择 Top 5

随机学习原先展示完整生成证据包，复习 Agent 展示多次检索累积的来源列表。这些材料包含选材上下文，不等同于最终题目的相关性排序。

现在展开“相关原文”时，使用卡片的最终题目重新检索；复习任务默认使用最新题目，多题时可切换对应题目。只显示相关度最高的最多 5 个不同原文段落，保留教材版本、章节、页码和完整文字。同一道题反复展开会复用已取得的结果；切换题目或教材后，迟到的请求不会覆盖新结果。原文不足时按实际数量显示，错误时可以重试，不影响卡片内容和 AI 解释。

## 检索方法

参考 [Sentence Transformers Retrieve & Re-Rank](https://www.sbert.net/examples/sentence_transformer/applications/retrieve_rerank/README.html) 和 [FlagEmbedding](https://github.com/FlagOpen/FlagEmbedding) 的两阶段流程，使用支持中文的 [BAAI/bge-reranker-base](https://huggingface.co/BAAI/bge-reranker-base)。

1. 沿用现有 BM25 与 BGE 向量的加权 RRF 混合检索，召回前 24 个分块。
2. 校验原文映射，补充同章节的可用相邻正文，排除不可用材料；按原文块身份及归一化全文去重。
3. 交叉编码器联合读取“题目、段落”，按照模型分数降序排列后选择最多 5 条。分数用于排序，不解释为事实正确率或答案充分性概率。
4. 长段落使用有重叠的 token 窗口，取最大窗口分数；所有窗口都参与评分，展示仍保留完整原文。总文本预算沿用 10,000 字。

模型 revision 固定为 `2cfc18c9415c912f9d8155881c133215df768a70`，使用 safetensors、本地文件和 SHA-256 校验，不执行远程模型代码。重排模型有独立资源清单，不改变 PDF 导入所绑定的模型版本。现有索引和生成证据包无需迁移。

展示检索通过 `rag_related_sources → desktop.related_sources → assemble(reranker=...)` 运行。普通检索、随机选材、LLM 生成及复习工具仍保留其上下文证据；原有引用编号和学习记录不会被新的 Top 5 覆盖。历史卡片按其原教材版本检索。

## 验证方法

```powershell
.\rag-service\.venv\Scripts\python.exe -m pytest rag-service/tests -q
node node_modules/vitest/vitest.mjs run
cargo test --manifest-path src-tauri/Cargo.toml --lib --offline --quiet
.\rag-service\.venv\Scripts\python.exe evals/textbook/evaluate_top5.py --data data --out tmp/textbook-eval/top5-reranker.json
```

离线评测使用现有 40 道教材题目，比较“原流程的前 5 个原文块”和“混合召回后重排的前 5 个原文块”，保证展示预算相同。标签并未穷尽所有相关段落，因此报告已标注原文的命中、召回与排名，不把这些指标称为精确率，也不宣称任意问题都能得到充分证据。这些题目已在先前评测中使用，本次作为回归集，不作为新的盲测集。

## 本次结果

40 题均返回 5 条不同原文；其中 34 题有原文标签，6 题为无原文 / 视觉证据场景，不计入下表分母。完整数据见 [评测报告](../evals/textbook/reports/top5-reranker.json)。没有调用 LLM API。

| 指标（34 道有标签题）       | 原流程前 5 条 | 重排后前 5 条 |
| --------------------------- | ------------: | ------------: |
| 首条命中标注原文            |       18 / 34 |       26 / 34 |
| 前 5 条至少命中一条标注原文 |       27 / 34 |       30 / 34 |
| 前 5 条包含全部标注原文     |       25 / 34 |       27 / 34 |
| 平均倒数排名 MRR@5          |         0.640 |         0.814 |
| 平均标注原文召回率          |        75.98% |        84.31% |

存在明确的局限：`x01`、`x04` 的首条标注排名下降，但全部标注原文仍在 Top 5；`x05` 漏掉了“存取时间”的定义段落。`t02`、`t06`、`t12`、`t19` 的既有可检索范围仍不包含目标段落。另对 10 道复合题试验了“原题加字面拆句取最高分”，新增 `x01`、`x07` 的证据缺失，因此未采用。最终使用完整题意评分，不针对题号或标准答案添加特例。

本机 CPU 上，模型已加载时的检索与重排中位耗时为 **6.56 秒**；独立桌面进程首次请求包含模型校验与加载，实测 **17.95 秒**。首次展开显示加载状态，同一道题在组件存续期间再次展开复用结果。BGE Reranker 约 1.1 GB 权重已在本机准备并校验，重排不增加 API 费用。

验证结果：

- 前端 116 项、Rust 126 项、Python RAG 69 项、评测脚本 23 项，共 334 项测试通过；原有 6 个需显式实机 / API 条件的入口保持默认忽略。
- 新测试覆盖先重排再取 5 条、重复原文、空结果、损坏模型、旧教材版本、长文本尾部、实际 tokenizer 配对格式、异步过期响应、失败重试和按题切换。
- 浏览器在 360 × 440、420 × 560 窗口下验证 5 条上限、无横向溢出、题目切换、原文跳转回调及 AI 解释。浏览器使用模拟数据；另通过真实 Python 桌面 stdin 协议验证本地模型、Top 5 返回和原 PDF 页渲染。
- TypeScript、ESLint、Ruff、Clippy、Vite 构建、Windows Release 构建及 `git diff --check` 通过。修改文件的 Prettier 检查通过；全库检查提示 48 个未改动文件的既有格式问题，未扩大改动范围。
