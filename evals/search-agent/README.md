# 智谱搜索 P0 验证

官方契约核对日期：2026-09-21。此目录的 Node 脚本用于接口探测，不是桌面搜索 Agent 实现。

## 已核实的接口契约

- 固定端点：`POST https://open.bigmodel.cn/api/paas/v4/web_search`，Bearer 鉴权。
- `search_query` 最多 70 字符；`search_intent=false` 直接执行搜索。
- 引擎：`search_std`、`search_pro`、`search_pro_sogou`、`search_pro_quark`。
- `count` 为 1–50；搜狗只接受 10 的倍数；文档没有列出夸克支持 `count` / 域名过滤，所以不向夸克发送这些参数。
- 时间范围：`oneDay`、`oneWeek`、`oneMonth`、`oneYear`、`noLimit`。摘要规格为 `medium` 或 `high`。
- `request_id` 是 6–64 字符的追踪 ID，文档未承诺幂等重试。
- 结果保留标题、URL、摘要、发布方、引用标识和可缺失的发布日期。不把请求创建时间当作资料日期。
- 余额不足可能是 HTTP 429 + `1113`，要与速率限制区分；`1701` 为并发上限，`1702` 为引擎服务不可用，`1703` 为没有有效结果。

依据：[官方接口](https://docs.bigmodel.cn/api-reference/工具-api/网络搜索.md)、[官方错误码](https://docs.bigmodel.cn/cn/api/api-code.md)。

官方当前搜索刊例单价：Std 0.01 元/次、Pro 0.03 元/次、搜狗与夸克各 0.05 元/次。账号余额、专属配额、实际扣费和优惠不能从刊例价格推断；搜索响应也不提供实际账单。[官方定价](https://docs.bigmodel.cn/cn/guide/start/pricing.md)

## 本地契约测试

```powershell
node --test evals/search-agent/contract.test.mjs
```

`fixtures/contract.json` 全部是明确标记的合成样例，不能用于证明真实账号可用。测试覆盖参数限制、强制搜索、错误分类、缺失日期、脱敏和响应大小限制。

## 环境变量方式的真实探测

```powershell
powershell -NoProfile -File scripts/verify-zhipu-search.ps1 -Live
```

读取当前进程或用户级 `ZHIPU_API_KEY`，最多 4 次 HTTP 尝试，不自动重试。分别探测 Pro、Std、无匹配查询及故意无效 Key。账号出错时提前停止。脱敏报告写到 Git 忽略的 `tmp/search-agent/`，不会保存凭据、请求头和原始错误消息。

“无结果”探测采用随机词和无效公开域名；若搜索引擎没有遵守过滤条件，应标记探测未通过，而不是伪造空结果样例。HTTP 成功只说明契约可用，内容相关性、发布时间与实际计费需单独核对。

## 当前用户选择

用户要求通过 APP 配置 Key，再进行 P0 真实验证。因此已提前开发最小凭据设置界面，与 DeepSeek / Kimi 同级；凭据只写 Windows Credential Manager。环境变量脚本是备用验证路径，无需用户重复配置环境变量。

P0 真实测试未通过前，不启动其余 P1 / P2 工作。进展见 [验证记录](../../docs/validation/search-agent-p0-setup.md)。
