## ADDED Requirements

### Requirement: 控制台必须为 request 提供 prompt diff
PrismTrace request inspector MUST 提供当前 request 相对于同一 session 内上一条 request 的 prompt diff，使用户能够直接检查 prompt 文本的变化，而不必手工比较两份 request body。

#### Scenario: 当前 request 存在上一条可比较 request
- **WHEN** 当前 request 所在 session 中存在时间上紧邻的上一条 request，且两者都能提取 prompt projection
- **THEN** 控制台返回 `available` 的 prompt diff，并展示上一条 request 引用和 diff 文本

#### Scenario: 当前 request 没有上一条 request
- **WHEN** 当前 request 是该 session 中的第一条 request
- **THEN** 控制台返回 `no_previous_request`，而不是伪造空 diff

### Requirement: prompt diff 必须只比较 prompt-bearing 文本
PrismTrace host MUST 从 request body 中提取 prompt-bearing 文本 projection，再基于 projection 做 diff，以避免工具定义和无关参数噪音淹没 prompt 变化。

#### Scenario: tools 和采样参数不得进入 prompt projection
- **WHEN** request body 中同时包含 messages、tools 和 sampling 参数
- **THEN** prompt projection 只包含 prompt-bearing 文本，不包含 tools / functions 定义和采样参数

#### Scenario: 常见文本字段可被提取到 projection
- **WHEN** request body 中包含 `system`、`instructions`、`messages[*].content` 或 `input`
- **THEN** host 将这些文本字段按稳定顺序渲染到 projection 中

### Requirement: 无法比较时必须返回显式状态
PrismTrace request inspector MUST 在无法生成 prompt diff 时返回显式状态，使用户不会把“不可比较”误解为“没有变化”。

#### Scenario: request body 无法生成 projection
- **WHEN** 当前 request 或上一条 request 的 body 为空、不是可解析 JSON，或不包含可提取的 prompt-bearing 文本
- **THEN** 控制台返回 `unavailable_projection`
