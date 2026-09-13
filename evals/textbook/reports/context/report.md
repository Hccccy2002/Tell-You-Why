# 真实教材评测报告

引用匹配率只校验定位；人工/AI 复核分开计数，缺失指标保持 null。

| 模式/分组          | 有证据题 |  Hit@5 | 证据召回率 |
| ------------------ | -------: | -----: | ---------: |
| hybrid/development |        5 | 100.0% |      93.3% |
| hybrid/regression  |       24 |  79.2% |      83.3% |

执行及复核结果：

```json
{
  "generation": {
    "planned": 9,
    "recorded": 9,
    "completed": 9,
    "answered": 6,
    "insufficient": 3,
    "failed": 0,
    "pending_or_interrupted": 0,
    "citation_substring_validity": 1,
    "duration_median_ms": 6890,
    "requests": 15,
    "requests_with_token_usage": 15,
    "reported_total_tokens": 47258
  },
  "agent": null,
  "quality_reviews": {
    "human": null,
    "ai": {
      "reviewed": 9,
      "planned": 9,
      "correct": 0.7777777777777778,
      "grounded": 1
    }
  }
}
```
