# 迭代 4 收口：Node CLI 真实 attach 与前台持续采集设计

日期：2026-04-18
状态：已完成（已合并）

## 1. 背景

PrismTrace 当前已经完成了迭代 4 的大部分请求采集链路：

- `HttpRequestObserved` IPC 协议已存在
- probe 已能观察高置信度 HTTP 请求并上报
- host 已能过滤、生成摘要并写入 request artifacts

但本轮还没有真正完成路线图里定义的两个收口目标：

- 把真实 attach 入口切到前台持续采集模式
- 落地真实插桩后端，替换 `NodeInstrumentationRuntime` 占位实现

当前 `--attach <pid>` 仍然走 `ScriptedAttachBackend`，只能做一次性 attach 演示，不会在 attach 成功后持续消费真实 probe IPC。与此同时，`NodeInstrumentationRuntime` 仍返回“not yet implemented”，导致 PrismTrace 还不能对一个真实运行中的目标完成不重启 attach 和请求采集。

## 2. 本轮目标

本轮只做“迭代 4 收口”，不额外展开到控制台 UI 或 response capture。

### 2.1 主验收

主验收目标收敛为：

- 对一个运行中的纯 Node CLI 目标进程执行 `--attach <pid>`
- 在不重启目标进程的前提下完成真实 attach
- attach 成功后进入前台持续采集模式
- 当目标进程发出模型请求时，CLI 即时打印摘要并把原始 payload 落盘

### 2.2 辅助探索

Electron 目标本轮不进入正式验收，只允许作为可行性探索对象：

- 可以记录 attach 可达性或失败模式
- 不承诺稳定采集
- 不把 Electron 支持写入本轮完成定义

## 3. 范围与非目标

### 3.1 本轮范围

- 让 `--attach <pid>` 进入真实前台持续采集模式
- 用真实运行时替换 `NodeInstrumentationRuntime` 占位实现
- 仅支持 `RuntimeKind::Node` 的最小真实闭环
- 复用现有 request capture、artifact 落盘和摘要渲染逻辑

### 3.2 明确非目标

- 不在本轮承诺 Electron attach/capture
- 不引入 response capture 或 stream capture
- 不引入 daemon、后台服务或跨命令 session 持久化
- 不改造当前 artifact 存储模型
- 不为 inspector 路径重做一套独立的 request capture 领域模型

## 4. 方案对比

### 4.1 Frida-first

直接实现一个通用动态注入后端，同时覆盖 Node CLI 与 Electron。

不采用原因：

- 对当前仓库跨度过大
- 会同时引入 native 注入、权限与多进程模型问题
- 会拖慢“先拿到一条真实请求”的主目标

### 4.2 Inspector-first

仅为运行中的纯 Node 进程实现 inspector attach 后端。

优点：

- 与主验收目标一致
- 实现面最小
- 可以最快把 `NodeInstrumentationRuntime` 从占位实现推进到真实可用实现

缺点：

- 对 Electron 的复用价值有限
- 后续大概率仍需第二套运行时

### 4.3 Hybrid

保留统一 `InstrumentationRuntime` 抽象，本轮只正式实现 Node inspector 运行时；Electron/Frida 的空间仅保留在接口边界，不在本轮交付。

采用原因：

- 兼容当前代码结构
- 本轮交付承诺仍然等价于 Inspector-first
- 不会把未来运行时方案写死进 host 业务逻辑

## 5. 选型结论

本轮采用 `Hybrid` 架构收敛，但交付承诺按 `Inspector-first` 执行：

- 本轮正式交付 `NodeInspectorRuntime`
- 通过它替换 `NodeInstrumentationRuntime` 的占位行为
- 上层 attach / request capture 逻辑尽量保持不变

核心原则是：

`本轮只替换“怎么把 probe 放进去”，不重写“probe 放进去之后怎么采”。`

## 6. 总体设计

### 6.1 架构边界

本轮仍沿用现有四段链路：

1. `main.rs` 接收 `--attach <pid>` 并进入 attach 路径
2. `attach.rs` 负责 attach 生命周期、握手和 listener 管理
3. `runtime.rs` 负责把 bootstrap probe 注入到目标 Node 进程
4. `request_capture.rs` 负责消费 `HttpRequestObserved`、过滤、写 artifact、输出摘要

其中本轮主要变化集中在第 1、2、3 段。

### 6.2 前台持续采集模式

`--attach <pid>` 将从“一次性 attach 快照输出”调整为“前台持续采集入口”：

1. host 继续运行现有 `discovery -> readiness -> attach` 控制流
2. attach 成功后打印 attached 摘要
3. host 不退出，而是进入前台事件循环
4. 前台循环持续消费 probe IPC
5. 命中请求后即时打印摘要并写 artifact
6. 收到 `DetachAck`、channel 断开或 heartbeat 超时后退出
7. 用户 `Ctrl+C` 结束 host 进程时，host 做 best-effort detach 清理

### 6.3 真实插桩后端

`NodeInstrumentationRuntime` 将由占位实现替换为真实可用的 Node inspector 运行时。

设计职责：

- 向目标进程发送 `SIGUSR1`，激活 Node inspector
- 发现该 pid 对应的 inspector 本地监听端口
- 获取 `webSocketDebuggerUrl`
- 建立 inspector 会话并确认连接到正确 pid
- 在目标进程内执行 bootstrap probe
- 把目标进程内的 probe 消息桥接回 host，继续表现成 line-oriented IPC 流

## 7. Node inspector 运行时设计

### 7.1 inspector 激活与端点发现

运行时对目标 pid 执行下述流程：

1. 发送 `SIGUSR1`
2. 轮询目标 pid 的本地 TCP 监听端口
3. 找到 inspector 对应监听地址后，请求 `/json/list`
4. 读取 `webSocketDebuggerUrl`
5. 建立 WebSocket 调试连接

本轮不使用“猜测默认端口属于谁”的策略，而是优先把监听端口归因到指定 pid，再继续连接，避免多进程环境下误连到别的 Node 实例。

### 7.2 连接正确性校验

建立 WebSocket 后，运行时先执行轻量表达式校验，例如读取目标进程的 `process.pid`，确保 inspector 会话确实附着到了用户指定的 pid，而不是某个其他 Node 进程。

如果 pid 不匹配，attach 必须失败并返回结构化错误。

### 7.3 probe 注入方式

现有 `bootstrap.js` 继续作为唯一 probe 脚本。

运行时在 inspector 会话里执行一段装载逻辑：

- 先注册一个 host 桥接函数入口
- 再执行 `bootstrap.js`
- 让 bootstrap 在当前进程内安装 hook、上报 bootstrap report、维持 heartbeat、处理 detach

本轮不为 inspector 单独维护第二套 probe 协议或第二份 probe 文件。

## 8. probe 消息桥接

### 8.1 问题

现有 probe 默认通过 `process.stdout.write(JSON + "\\n")` 向 host 发消息。

但对“attach 到已运行进程”的 inspector 模式，host 并不天然持有目标进程 stdout，因此无法继续依赖现有 stdout reader 作为真实 IPC 通道。

### 8.2 方案

把 probe 的发消息能力抽象为“可替换 emitter”：

- 默认模式：继续写 `process.stdout`
- inspector 模式：改为调用宿主注入的桥接函数，例如 `globalThis.__prismtraceEmit(jsonLine)`

桥接函数职责：

- 接收 bootstrap 生成的单行 JSON
- 通过 inspector 会话把消息发回 host
- 在 host 侧继续包装成现有 `BufRead` 风格输入，让 `IpcListener` 无需知道底层 transport 变化

### 8.3 兼容性要求

- 现有 scripted runtime 和 probe 测试不能因为引入 emitter 抽象而回退
- `bootstrap.js` 在没有桥接函数时，必须自动退回 stdout 模式
- probe 侧观测异常仍然必须吞掉，不能影响原始请求语义

## 9. 失败处理

本轮所有失败都必须表现成结构化 attach failure，而不是裸日志或静默退出。

主要失败场景：

- 目标进程不是纯 Node CLI，或当前 readiness 不支持
- 目标进程无法响应 `SIGUSR1`
- inspector 监听端口发现失败
- `/json/list` 请求失败或未返回可用 `webSocketDebuggerUrl`
- inspector WebSocket 建连失败
- inspector 会话校验出的 `process.pid` 与目标 pid 不一致
- bootstrap 未返回 `BootstrapReport`
- attach 后 heartbeat 超时
- best-effort detach 失败

错误分类尽量复用现有 `AttachFailureKind`；只有在现有分类无法表达时，才补最小新增类型。

## 10. 文件边界

### 10.1 重点修改文件

- `crates/prismtrace-host/src/main.rs`
  - 把 `--attach` 从快照输出改成真实前台采集入口
- `crates/prismtrace-host/src/runtime.rs`
  - 实现真实 `NodeInstrumentationRuntime`
  - 必要时把 inspector 会话细节拆出独立辅助结构
- `crates/prismtrace-host/src/attach.rs`
  - 保持 attach controller 和 listener 管理
  - 支撑 attach 成功后继续消费 probe 事件
- `crates/prismtrace-host/probe/bootstrap.js`
  - 提炼 emitter 抽象
  - 保持 stdout 模式向后兼容

### 10.2 尽量不改职责的文件

- `crates/prismtrace-host/src/request_capture.rs`
  - 继续负责 host 侧过滤、artifact 落盘与 CLI 摘要
- `crates/prismtrace-core/src/lib.rs`
  - 除非运行时桥接被证明需要新的 IPC 消息，否则不在本轮扩大协议面

## 11. 测试与验收

### 11.1 单元与集成测试

本轮至少需要覆盖：

- `main.rs` 的 `--attach` 前台路径不会再退化成脚本化快照输出
- `runtime.rs` 中 Node inspector 运行时的端点发现、错误映射和桥接逻辑
- `bootstrap.js` 在 stdout 与 bridge emitter 两种模式下都能发出相同格式的消息
- `attach.rs` 与 `request_capture.rs` 在真实 runtime 接口下保持状态机一致性

### 11.2 黑盒验收

主验收以一个真实运行中的纯 Node CLI 目标为准：

- 目标进程先启动，再由 PrismTrace attach
- attach 成功后无需重启目标
- 目标进程发起至少一条真实模型请求
- CLI 看到一条 `[captured] ...` 摘要
- `.prismtrace/state/artifacts/requests/` 中出现对应 artifact 文件

Electron 本轮只记录探索结论，不作为失败判定条件。

## 12. 风险与收口

### 12.1 已知风险

- Node inspector 的启用依赖目标运行时配置，部分进程可能关闭或限制 `SIGUSR1`
- inspector bridge 会引入一层额外 transport，若桥接实现不稳，可能导致 heartbeat 或消息顺序问题
- 已运行进程的 attach 清理比脚本化 runtime 更容易残留状态

### 12.2 收口策略

- 本轮只承诺纯 Node CLI 主路径
- 复用现有 probe 与 request capture，避免同时改动过多层
- 将 Electron 能力明确降级为探索结论，避免假完成

## 13. 完成定义

本轮可以被视为完成，当且仅当以下条件同时满足：

- `--attach <pid>` 对纯 Node CLI 目标进入真实前台持续采集模式
- `NodeInstrumentationRuntime` 不再是占位实现
- attach 后能在不重启目标的前提下看到至少一条真实模型请求摘要
- 对应 request artifact 成功落盘
- 现有 scripted attach / bootstrap / request capture 测试未回退
