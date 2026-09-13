# 真实教材评测报告

引用匹配率只校验定位；人工/AI 复核分开计数，缺失指标保持 null。

| 模式/分组           | 有证据题 |  Hit@5 | 证据召回率 |
| ------------------- | -------: | -----: | ---------: |
| dense/development   |        5 | 100.0% |      76.7% |
| dense/regression    |       24 |  62.5% |      70.8% |
| hybrid/development  |        5 | 100.0% |      93.3% |
| hybrid/regression   |       24 |  79.2% |      79.2% |
| keyword/development |        5 | 100.0% |      93.3% |
| keyword/regression  |       24 |  62.5% |      75.0% |

执行及复核结果：

```json
{
  "generation": {
    "planned": 8,
    "recorded": 8,
    "completed": 6,
    "answered": 4,
    "insufficient": 2,
    "failed": 2,
    "pending_or_interrupted": 0,
    "citation_substring_validity": 1,
    "duration_median_ms": 8140.5,
    "requests": 20,
    "requests_with_token_usage": 20,
    "reported_total_tokens": 72645
  },
  "agent": {
    "state": "completed",
    "completed": true,
    "saved_questions": 1,
    "graded_questions": 1,
    "duration_ms": 19205,
    "model_requests": 7,
    "note": "脚本选择固定选项，完成率不代表真实用户掌握或题目正确率。"
  },
  "quality_reviews": {
    "human": null,
    "ai": {
      "reviewed": 6,
      "planned": 8,
      "correct": 1,
      "grounded": 1
    }
  }
}
```
