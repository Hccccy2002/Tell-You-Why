# 复习 Agent 质量评测

这套评测通过真正的 Agent 循环、工具分发和 SQLite 持久化执行，不直接伪造最终任务状态。`cases.json` 使用固定的微型教材样例，包含答对、答错、空检索、换词重试、检索故障、章节隔离、无效参数、失败恢复、禁止代答、材料注入十个场景。

## 可重复的流程回归

在项目根目录执行（需要 Rust、Python）：

```powershell
cargo test --manifest-path src-tauri/Cargo.toml review_eval_scripted --offline -- --ignored --nocapture
python -m unittest discover -s evals/review-agent -p test_evaluate.py -v
python evals/review-agent/evaluate.py --runs tmp/review-agent-eval/scripted-runs.json
```

输出位于 `tmp/review-agent-eval/`：`scripted-runs.json` 是逐场景运行结果；`report.json` 和 `report.md` 是评分报告；`annotations-template.json` 用于人工复核。

`scripted` 模式用受控模型回复验证编排行为，包含错误工具调用和一次模型故障。通过率仅代表流程回归通过率，不能写成模型准确率或真实 PDF 的 RAG 检索准确率。耗时包含本地数据库操作，模拟模型不会产生实际 API 费用。

## 可选真实模型评测

真实模型评测默认跳过。使用应用中已配置并验证的通道，显式指定**配置数据库的副本**与通道名称后执行：

```powershell
$env:TELLWHY_REVIEW_EVAL_PROFILE_DB = 'D:\your-eval\app-copy.db'
$env:TELLWHY_REVIEW_EVAL_PROVIDER = 'deepseek' # 或 kimi
cargo test --manifest-path src-tauri/Cargo.toml review_eval_live --offline -- --ignored --nocapture
python evals/review-agent/evaluate.py --runs tmp/review-agent-eval/live-runs.json --out tmp/review-agent-eval/live
```

`--offline` 只限制 Cargo 获取依赖，**不会阻止真实模型 API 请求**。此模式使用系统凭据库中的现有密钥，会产生模型费用；向已选通道发送公开的样例文本，测试数据与答题记录存入临时数据库。不会读取个人 PDF 或修改实际学习状态。评测通道的区域使用选中配置中的值。

`live_model_controlled_tools` 仍使用受控教材检索，真实模型自主决定工具序列。工具故障、空结果、跨章节结果和材料注入来自受控环境；无效参数、代答、模型网络故障仅在 scripted 模式主动注入，真实模型是否出现这些行为由运行结果决定。它不是生产 PDF 检索性能基准，也不是攻击鲁棒性的完整证明。

## 评分与人工复核

自动评分检查任务完成、必要工具使用、实际提交的答案、引用编号、章节范围和重复题目。按模型给定答案键核对用户选项是流程检查，不能证明答案键在知识上正确。

复制 `annotations-template.json` 为 `annotations.json`，对照 `*-runs.json` 中题目、参考答案、解释和原文，填写：

- `rater`：复核人。
- `factual_correct`：题目、参考答案及解释是否正确；无法判断填 `null`。
- `grounded`：引用原文是否确实支持该题的参考答案和解释；无引用的题目填 `null`。
- `notes`：错误原因或判定依据。

```powershell
python evals/review-agent/evaluate.py --runs tmp/review-agent-eval/live-runs.json --annotations tmp/review-agent-eval/live/annotations.json --out tmp/review-agent-eval/live/reviewed
```

人工指标只统计实际标注的题目，同时报告复核题数。没有标注时指标为 `null`；记录通过运行 ID、题目 ID 和内容指纹关联，旧标注不能混用于新模型输出。报告须同时说明数据集版本、提示词版本、模型、样本量、执行模式和人工复核覆盖范围。
