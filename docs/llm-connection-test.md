# LLM Connection Test Notes

## 背景

Connection test 已经两次在 MiniMax-M3 上误报：

1. 返回 `provider returned an empty response`。
2. 返回 `provider request timed out after 15000ms`。

两次共同点是：旧实现用一次 Chat Completion 来判断连接，请求近似为
`Reply with OK.` 且 `max_tokens = 1`。MiniMax-M3 是 reasoning-style 模型，
第一个输出 token 可能消耗在 reasoning 上，因此 HTTP 200 但可见正文为空；
模型也可能因为内部 reasoning 或 provider 负载导致一次小请求耗时超过 15 秒。

2026-09-22 的验证记录：

- `GET /v1/models` 直连返回 401，耗时约 0.23s，说明网络和 endpoint 可达。
- 同一配置直接调用 Chat Completion，`max_tokens = 1` 时返回 200，但正文为空。
- `max_tokens = 64` 时有一次约 1.5s 返回正文，也有一次 reasoning 耗时超过 14s。
- 因此“能否生成文本”不适合作为最低层级的 connection check。

## 当前行为

`RigLlmClient::test_connection` 现在调用 OpenAI-compatible 的
`GET /models`，只验证：

1. Base URL 可达。
2. API key 可通过 provider 鉴权。
3. Provider API 响应在 30 秒内返回。

它不再要求模型生成正文，因此 reasoning token 消耗完 `max_tokens` 不会误判为
connection failure。这也不会验证模型名是否可用；模型名错误、上下文超限、
模型拒绝请求等会在 Ingest 或 Query 阶段暴露，并保留在 workflow audit 中。

## 排查步骤

1. 确认 Base URL 通常是 OpenAI-compatible 根路径，例如
   `https://api.minimaxi.com/v1`，不要画蛇添足地拼接 `/chat/completions`。
2. 确认模型名和 provider 文档完全一致；connection test 不校验模型推理。
3. 如果 Test connection 失败，先用不带 key 的 `GET /models` 看是否快速返回
   401/404；这可以区分网络/DNS 问题。
4. 再用正确 key 调用 `GET /models`；非 200 时优先检查 API key、endpoint 和代理。
5. 如果 connection test 成功但 Ingest 失败，检查 `.wiki-db/audit/<run-id>/`
   里的 request/response artifact、schema validation decision 和 provider 错误。

不要在 issue 或日志中粘贴 API key。Provider 返回的 request id 可以记录，便于向
provider 排查。
