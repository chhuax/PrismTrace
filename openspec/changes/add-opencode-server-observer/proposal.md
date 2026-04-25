# Proposal: add-opencode-server-observer

## Why

`opencode` 的实机验证已经表明：

- 当前 `SIGUSR1 + inspector` attach 路线会把 live `opencode` 打死
- `opencode` 自身已经提供官方 `server + attach(url) + export + plugin/event` 能力

如果继续把 `opencode` 当成一个等待 Bun attach 兼容的目标，会长期阻塞产品落地。

因此需要为 `opencode` 新增一条官方 observer 路线。

## What Changes

- 在 host 中新增 `OpencodeServerSource`
- 提供最小 CLI/host observer 入口
- 第一版优先接入：
  - health
  - session list
  - session export
  - global event
- 将 `opencode` 高层运行时数据归一化到统一 observer 事件层

## Impact

受影响 spec：

- 新增：`opencode-server-observer`

受影响模块：

- `prismtrace-host`
- 统一 observer 接口层

## Out of Scope

- 本 change 不要求实现 Bun attach
- 不要求获取原始后端 HTTP 报文
- 不要求这一轮统一控制台 UI
