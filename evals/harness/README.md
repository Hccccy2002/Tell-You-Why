# Harness v1 故障注入

这套测试经过真实的 Rust Agent 循环、工具分发、临时 SQLite、上下文组装及完成验收。模型和教材使用可控实现，每次试验创建独立数据库，不读写个人学习记录、不发起真实模型 API 请求。

## 运行

在项目根目录执行：

```powershell
$env:TELLWHY_HARNESS_OUTPUT = Join-Path (Get-Location) 'tmp/harness-v1'
cargo test --manifest-path src-tauri/Cargo.toml --lib --offline harness_fault_injection_suite -- --nocapture
```

输出为 `tmp/harness-v1/report.json`。不设置环境变量时仍运行测试，只在终端报告结果。场景定义通过 `include_str!` 随测试编译，报告记录数据集版本与 SHA-256、Harness 和提示词版本、运行时间、试验结果及安全投影后的执行追踪。

单个试验断言失败后仍运行其他试验并导出报告，最终测试返回失败；报告的 `passed` 为实际通过数，失败细节见测试输出。报告不导出原始消息、模型隐藏推理或待答题答案键。

## 场景

| 场景                    | 注入与检查                                     | 预期                                         |
| ----------------------- | ---------------------------------------------- | -------------------------------------------- |
| `transient_retry`       | 一次模型网络错误                               | 有界重试后完成                               |
| `no_progress`           | 更换调用 ID、连续三次相同检索                  | 停止，不记录答题                             |
| `token_limit`           | 剩余用量不足以预留请求                         | 请求发送前停止，零模型请求                   |
| `context_compaction`    | 插入长历史                                     | 有界上下文完成，原始历史不变                 |
| `premature_finish`      | 模型未出题就宣称完成                           | 修正后实际作答、记分并完成                   |
| `tool_timeout`          | 首次检索一直未返回                             | 超时后换词查询并完成                         |
| `checkpoint_redelivery` | 记分提交后模型故障、待执行工具处重启、重复投递 | 数据库重开后恢复，提交与工具重放均只累计一次 |
| `inflight_pause`        | 工具等待期间请求暂停                           | 结束等待，保留待执行步骤                     |

默认每个场景运行三次，共 24 次。这是确定性流程与状态一致性回归，通过率不能用于声明真实模型可靠率；三次也不是统计显著性证明。测试中的工具超时为挂起的异步实现，不证明能终止 Python 子进程。

`content_quality` 固定为 `not_assessed`。语义正确性、实际 PDF 检索质量、真实供应商兼容性与成本需要独立实测。

其他边界由常规 Rust 测试覆盖，包括预算耗尽、超时与暂停、非法配置、未知用量预留、原文 Unicode 分页、孤立工具结果、提前完成修正上限、缺失记分事件、真实提交伪造、教材引用要求及错误模型消息结构。

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --lib --offline
npm.cmd run test
```

原有十场景受控评测入口保持可用，见 [复习 Agent 评测](../review-agent/README.md)。开发阶段和整体验收结果见 [Harness 开发记录](../../docs/harness-v1-development.md)。
