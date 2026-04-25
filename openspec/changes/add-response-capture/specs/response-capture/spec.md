## 变更概述

`response-capture` 能力为 PrismTrace 增加对模型相关 HTTP response 的稳定观测能力，使用户在 attach 到运行中的 Node / Electron AI 应用后，不只知道“它发出了什么 request”，也能知道“它收到了什么 response”。

本次变更聚焦于单次 exchange 的闭环：response 事件、request-response 关联、最小 response artifact、CLI 摘要输出，以及错误响应和大 body 下的安全降级。它不要求在这一轮完成 stream replay、完整 response inspector 或 session reconstruction。

## 术语说明

- response artifact：host 将已观察到的 response 事实写入本地状态目录后的结构化记录。
- exchange：一次 request 与其对应 response 的最小闭环单元，本轮通过 `exchange_id` 关联。
- terminal response：一次请求生命周期结束后形成的最终 response 事实，不等价于流式 chunk 级时间线。

## ADDED Requirements

### Requirement: 已 attach 的目标必须支持 response 事实采集
PrismTrace host MUST 在已 attach 的目标发生模型相关 HTTP response 时，采集该 response 的核心事实，而不只停留在 request 侧可见性。

#### Scenario: 主路径请求产生 response 事件
- **WHEN** 一个已 attach 的目标向模型 provider 发出受支持的请求并收到 response
- **THEN** host 收到结构化 response 事件，并将其纳入当前观测链路

#### Scenario: 非模型相关 response 不产生误导性记录
- **WHEN** 一个已 attach 的目标产生与模型 provider 无关的普通 HTTP response
- **THEN** host 不把该 response 误记为模型相关 response artifact

### Requirement: response capture 必须支持 request-response 的最小闭环
PrismTrace host MUST 使用稳定关联键把单条 response 与其对应 request 关联起来，使后续产品面和分析层可以在 exchange 粒度上消费这些事实。

#### Scenario: response 与 request 共享同一 exchange 标识
- **WHEN** 一个 request 已被捕获，随后收到对应 response
- **THEN** request 与 response 使用同一个 `exchange_id` 形成最小闭环

#### Scenario: response 复用 request 的 provider 上下文
- **WHEN** host 已在 request 路径识别出该 exchange 的 provider hint
- **THEN** response artifact 优先复用该 provider 上下文，而不是重新生成不一致的归类结果

### Requirement: response capture 必须持久化最小 response artifact
PrismTrace host MUST 将已识别的 response 写入本地状态目录，使该结果不仅可在当前 CLI 会话中观察，也可被后续控制台和详情页复用。

#### Scenario: 已识别 response 被写入 artifacts 目录
- **WHEN** host 成功识别并接收一条模型相关 response
- **THEN** `.prismtrace/state/artifacts/responses/` 下出现对应的结构化 artifact，至少包含状态码、时间信息、目标信息和关联键

#### Scenario: response 被输出为最小 CLI 摘要
- **WHEN** host 在前台 attach 持续采集模式下接收到一条已识别 response
- **THEN** 用户在 CLI 中看到一条可读摘要，而不是只能依赖事后翻阅 artifact 文件

### Requirement: response capture 必须在异常正文和大 body 下安全降级
PrismTrace host MUST 在 response body 过大、不可文本化或包含敏感头字段时，优先保证链路稳定与安全，而不是强求完整正文保真。

#### Scenario: 大 body 被截断但链路继续工作
- **WHEN** 一个已识别 response 的正文超过当前允许的安全体积上限
- **THEN** host 允许只持久化截断后的正文，并明确标记该 artifact 为截断结果

#### Scenario: 不可文本化 body 不会破坏主链路
- **WHEN** 一个已识别 response 的正文无法安全转换为文本
- **THEN** host 仍保留状态码、头部与时间信息，并以空正文或等价安全降级方式完成 artifact 写入

#### Scenario: 敏感头字段被脱敏后再持久化
- **WHEN** 一个已识别 response 包含 cookie 或等价敏感头字段
- **THEN** host 在 artifact 中只写入脱敏后的头部信息，而不写入原始敏感值

## MODIFIED Requirements

## REMOVED Requirements

## RENAMED Requirements

## 非目标与边界

- 本次不要求完成 stream chunk 级时间线回放。
- 本次不要求完成完整 response inspector 或原始正文高保真浏览体验。
- 本次不要求完成 session reconstruction、tool visibility 或 failure attribution。
- 本次不要求把 provider-specific 的 usage、finish reason、tool calls 全部归一化进稳定 schema。