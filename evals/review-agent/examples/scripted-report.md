# 复习 Agent 评测报告

执行模式：`scripted`。
受控微型教材场景，非真实 PDF 检索基准。scripted 仅验证编排；人工标注指标只适用于已评审输出。

数据集：review-eval-v1；提示词：review-agent-v1。
模型配置：deepseek-v4-flash（scripted 使用模拟回复）。
流程通过：10 / 10。
题目：10，带原文编号：9。

- 事实正确率：未评审（人工评审 0 题）
- 引用支持率：未评审（人工评审 0 题）

模型调用 64 次，工具调用 54 次。
已报告用量：1890 tokens，覆盖 63 次请求（scripted 用量为模拟值）。
场景耗时中位数 206.0 ms，P95 241 ms。
耗时包含本地持久化、检索及答题模拟；scripted 耗时不代表线上模型延迟。

| 场景                   | 结果 | 未通过检查 |
| ---------------------- | ---- | ---------- |
| basic-correct          | 通过 | —          |
| basic-wrong            | 通过 | —          |
| requery                | 通过 | —          |
| no-evidence            | 通过 | —          |
| retrieval-error        | 通过 | —          |
| scope-isolation        | 通过 | —          |
| invalid-arguments      | 通过 | —          |
| resume-once            | 通过 | —          |
| do-not-answer-for-user | 通过 | —          |
| injected-source        | 通过 | —          |
