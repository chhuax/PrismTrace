## Why

`PrismTrace` 现有能力主要建立在 `attach + probe` 路线上，适用于 Node / Electron 目标。但对 `Codex.app`，这条路线已经被证明不安全：当前 `SIGUSR1` inspector 唤醒方式会导致 live `Codex` 崩溃。

与此同时，`Codex` 已经确认存在官方接入面：

- `Codex App Server`
- 本地 IPC socket
- `codex app-server proxy`

这意味着 `Codex` 不应继续被当作“attach 兼容性问题”，而应被作为一个新的、官方支持的观测后端来接入。

因此需要新增 `add-codex-app-server-observer`，把 `PrismTrace` 对 `Codex` 的接入路线从危险 attach 改成官方 observer 路线。

## What Changes

- 为 `Codex` 新增一条独立于 attach 的官方观测后端接入方案
- 定义第一版最小事件面：`thread / turn / item / tool / approval / hook / plugin / skill / app`
- 先交付最小 CLI/host 验证入口，而不是直接扩展复杂 UI
- 保持现有 Node / Electron attach 路线不变，避免混线

## Capabilities

### New Capabilities

- `codex-app-server-observer`: 通过 `Codex App Server + IPC socket` 读取 `Codex` 的高层运行时事件

### Modified Capabilities

- `attach-readiness`: 继续对 `Codex` 保持“不可 attach”或等价保守表达，避免误导用户走危险 attach 路线

## Impact

- 影响代码：集中在 `crates/prismtrace-host`，以新增 `Codex` observer 模块为主
- 影响系统行为：`Codex` 将拥有新的官方观测入口，但不会进入现有 attach 控制器
- 影响文档：新增 `add-codex-app-server-observer` story 与 OpenSpec 文档
- 边界说明：本次不交付 `Codex -> 模型后端` 的原始 HTTP 报文抓取，也不先交付复杂控制台 UI
