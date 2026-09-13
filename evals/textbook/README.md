# 真实教材评测

`dataset.json` 固定《计算机组成原理》PDF、知识库版本、原文块与问题。
40 题涵盖单段、多段、概念辨析、教材外问题和目前无法解析的图示。
证据页码为 PDF 物理页码，从 1 开始；引用是非穷尽标注。

- 24 道旧检索探针作为 regression，不能宣称是独立测试集。
- development 8 题用于分析和调整。
- validation 8 题不用于调参，选定方案后再比较。
- 参考答案由 AI 对照原 PDF 的 17 个相关页面和原文块核验，**没有人工复核**。
  `review` 保留复核入口；人工确认后填写姓名/标识、日期和 method=human。
- 被 OCR 质量策略排除的正确原文仍保留为证据缺口，不能从分母删除来提高成绩。
- 本集规模小、同一教材、部分问题相近，结果不能代表泛化能力。

校验命令（从项目根目录执行）：

```sh
python -m unittest discover -s evals/textbook
python evals/textbook/dataset.py --data data --out tmp/textbook-eval/dataset-validation.json
```

`dataset-lock.json` 记录规范化 JSON 的 SHA-256。修改标注必须明确升级版本并重新冻结，
不能在对比实验期间修改问题、参考答案或证据。校验包括原 PDF、manifest、SQLite
哈希、段落定位、引用和原文哈希；校验通过只证明标注能追溯，不证明答案质量。

## 真实检索与生成

在装有项目 Python 依赖的环境运行；Windows 通常使用 `rag-service/.venv/Scripts/python.exe`：

```sh
python evals/textbook/retrieve.py --out tmp/textbook-eval/baseline.json
python evals/textbook/score.py --retrieval tmp/textbook-eval/baseline.json --out tmp/textbook-eval/baseline-report
```

默认只运行 development/regression 共 32 题，分别计算 keyword/dense/hybrid。
验证 PDF、索引及查询模型文件哈希之后，统计 Hit@5、全部证据命中、非穷尽块标签的
Recall@5、MRR@5、最终证据包召回率和热查询延迟；冷启动单独记录。
负例不进入召回率分母，也不会因为搜到了材料而算作成功。索引外原文仍进入正例分母。
评测输出不可覆盖；比较时使用新目录。完整证据包只保存在忽略跟踪的 `tmp/` 下。

真实模型使用已有 DeepSeek/Kimi 通道和 Windows 凭据库，调用现有 `rag::generate`
及复习 Agent。需要明确同意向对应官方 API 发送本教材摘录，可能产生费用。
不要在命令、报告或环境变量中放入 API Key。设置以下环境变量后运行忽略测试：

```powershell
$env:TELLWHY_TEXTBOOK_PROFILE_DB = 'D:/Tell-You-Why/tmp/textbook-eval/profiles.db'
$env:TELLWHY_TEXTBOOK_PROVIDER = 'deepseek'
$env:TELLWHY_TEXTBOOK_INPUT = 'D:/Tell-You-Why/tmp/textbook-eval/baseline.json'
$env:TELLWHY_TEXTBOOK_OUTPUT = 'D:/Tell-You-Why/tmp/textbook-eval/live-baseline'
cargo test --manifest-path src-tauri/Cargo.toml --lib textbook_eval_live -- --ignored --nocapture
```

profile 数据库应为当前设置的 SQLite 一致性快照；WAL 存在时用 SQLite backup，
不能只复制主数据库。只有设置记录，Key 仍通过当前 Windows 用户的凭据库读取。
所有学习记录写入输出目录下的临时数据库。默认 8 道回答加 1 次真实教材 Agent
流程；`TELLWHY_TEXTBOOK_CASES` 可指定逗号分隔的题号，
`TELLWHY_TEXTBOOK_SKIP_AGENT=1` 可只运行回答比较。**每个输出目录总计最多 40 次请求**。
参考答案及标注不会放入模型生成输入。

`requests.json` 在请求发出前保存状态，保存耗时、HTTP 状态及服务商报告的 token；
不保存 Key、提示词、原始响应或模型思考。`runs.json` 记录逐题结果和 Agent 执行追踪。
相同输入可以继续尚未开始的题；已开始但未完成的题保持中断状态，避免悄悄重复计费。
输入、题号、模型或提示词版本变化不能复用原输出目录。网络错误会停止该轮执行。

```sh
python evals/textbook/score.py --retrieval tmp/textbook-eval/baseline.json --runs tmp/textbook-eval/live-baseline/runs.json --out tmp/textbook-eval/live-report
```

输出报告和 `review-template.json`。复核者对照问题、参考答案、原文及实际输出，
填写 `method`（human/ai）、`reviewer`、`correct`、`grounded` 与具体理由，
再使用 `--annotations` 计分。结果指纹防止旧复核用于新生成结果。
引用字符串存在只代表定位有效；正确性与证据支持度分别计分，人工与 AI 分开报告，
没有复核则为 null。Agent 使用固定脚本选项；其完成率不代表用户掌握度或题目正确率。

`compare.py --before <baseline.json> --after <candidate.json> --out <comparison.json>`
比较同一模式、题号与分组；标注、教材、索引或模型漂移会报错，召回下降退出码为 1。
完整结果与限制见 [本次验收](../../docs/textbook-agent-roadmap.md)。

长期复习集成入口可设置 `TELLWHY_TEXTBOOK_ONLY_AGENT=1` 和
`TELLWHY_TEXTBOOK_MEMORY_SEED=<上轮临时agent.db>`。输入必须是已关闭且没有 WAL 的评测数据库。
脚本复制后升级迁移，在**副本**中模拟到期，不改变原始分数；验证同一知识点次数累积、
下次排期与到期队列。此运行记录带 `due_time_simulated=true`，不能描述为真实长期用户研究。
