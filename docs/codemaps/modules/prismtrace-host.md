# Module Identity

`prismtrace-host`

## Why It Exists

作为当前 PrismTrace 的顶层运行时入口，负责把 workspace root、本地状态初始化、process discovery、attach readiness 和本地 CLI 行为串起来。

## Main Entrypoints

- `src/main.rs`
- `bootstrap()`
- `collect_host_snapshot()`
- `discovery_report()`
- `collect_readiness_snapshot()`
- `readiness_report()`
- `discovery::discover_current_process_targets()`

## Core Flows

### Bootstrap

`main.rs` 先调用 `bootstrap()`，构造 `AppConfig` 和 `StorageLayout`，初始化 `.prismtrace/state`。

### Discovery

如果带 `--discover` 参数，host 使用 `PsProcessSampleSource` 读取 `ps -axo pid=,comm=`，生成 `ProcessSample`，然后映射成 `ProcessTarget` 并渲染为报告。

### Readiness

如果带 `--readiness` 参数，host 会先执行 discovery，再将候选 `ProcessTarget` 交给 `readiness` 模块进行保守判断，输出 `supported / unsupported / permission_denied / unknown` 结果和原因说明。

### Testing

host 测试主要覆盖三块：

- bootstrap 不回退
- discovery service 能把 sample 转成 target
- discovery report 能稳定输出 runtime label 和 pid
- readiness service 能稳定输出状态与原因说明

## Internal And External Dependencies

- 依赖 `prismtrace-core`
- 依赖 `prismtrace-storage`
- 真实 discovery 依赖系统 `ps` 命令

## Edit Hazards And Debugging Notes

- 这里容易膨胀成“所有东西都往 host 放”，后续加 attach / API / UI 之前要继续拆清模块边界
- 当前 `--discover` 只是最小本地入口，不要把它误认为最终产品 surface
- 当前 `--readiness` 也只是本地演示入口，不等于最终的目标选择体验
