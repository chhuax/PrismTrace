# Module Identity

`prismtrace-host`

## Why It Exists

作为当前 PrismTrace 的顶层运行时入口，负责把 workspace root、本地状态初始化、process discovery、observer source 接入和本地 CLI / console 行为串起来。

## Main Entrypoints

- `src/main.rs`
- `bootstrap()`
- `collect_host_snapshot()`
- `discovery_report()`
- `discovery::discover_current_process_targets()`
- `src/codex_observer.rs`
- `src/console/mod.rs`

## Core Flows

### Bootstrap

`main.rs` 先调用 `bootstrap()`，构造 `AppConfig` 和 `StorageLayout`，初始化 `.prismtrace/state`。

### Discovery

如果带 `--discover` 参数，host 使用 `PsProcessSampleSource` 读取 `ps -axo pid=,comm=`，生成 `ProcessSample`，然后映射成 `ProcessTarget` 并渲染为报告。

### Observer Sources

如果带 `--codex-observe` 参数，host 会连接 `Codex` 的官方 observer 面，读取高层运行时事件，并把这些事件写入 artifacts 供本地控制台消费。

### Console

如果带 `--console` 参数，host 会聚合本地 request/session/observer artifacts，启动 observer-first 的本地控制台。

### Testing

host 测试主要覆盖三块：

- bootstrap 不回退
- discovery service 能把 sample 转成 target
- discovery report 能稳定输出 runtime label 和 pid
- observer source 能稳定输出结构化事件
- console API / 页面 shell 能稳定暴露当前控制台契约

## Internal And External Dependencies

- 依赖 `prismtrace-core`
- 依赖 `prismtrace-storage`
- 真实 discovery 依赖系统 `ps` 命令

## Edit Hazards And Debugging Notes

- 这里容易膨胀成“所有东西都往 host 放”，后续加 observer source / API / UI 之前要继续拆清模块边界
- 当前 `--discover` 只是最小本地入口，不要把它误认为最终产品 surface
- 当前控制台已经转向 observer-first，不要把历史 attach 心智再带回 host surface
