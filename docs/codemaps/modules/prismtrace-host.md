# Module Identity

`prismtrace-host`

## Why It Exists

作为当前 PrismTrace 的顶层运行时入口，负责把 workspace root、本地状态初始化、process discovery、attach readiness、attach controller 和本地 CLI 行为串起来。

## Main Entrypoints

- `src/main.rs`
- `src/attach.rs`
- `bootstrap()`
- `collect_host_snapshot()`
- `discovery_report()`
- `collect_readiness_snapshot()`
- `readiness_report()`
- `collect_attach_snapshot()`
- `attach_snapshot_report()`
- `discovery::discover_current_process_targets()`

## Core Flows

### Bootstrap

`main.rs` 先调用 `bootstrap()`，构造 `AppConfig` 和 `StorageLayout`，初始化 `.prismtrace/state`。

### Discovery

如果带 `--discover` 参数，host 使用 `PsProcessSampleSource` 读取 `ps -axo pid=,comm=`，生成 `ProcessSample`，然后映射成 `ProcessTarget` 并渲染为报告。

### Readiness

如果带 `--readiness` 参数，host 会先执行 discovery，再将候选 `ProcessTarget` 交给 `readiness` 模块进行保守判断，输出 `supported / unsupported / permission_denied / unknown` 结果和原因说明。

### Attach

如果带 `--attach <pid>` 参数，host 会先执行 discovery 和 readiness，找到目标 pid 对应的 readiness 结果，再通过 `attach` 模块中的受控 backend 发起最小 attach 流程，并输出结构化 attach session 报告。

当前这条 attach path 仍然是一次性的 CLI demo：controller 生命周期只存在于单次命令执行期间，还没有跨命令维持 active session，也没有对外暴露 detach/status surface。长期运行的 host attach session 管理会放到后续迭代里补齐。

### Testing

host 测试主要覆盖三块：

- bootstrap 不回退
- discovery service 能把 sample 转成 target
- discovery report 能稳定输出 runtime label 和 pid
- readiness service 能稳定输出状态与原因说明
- attach controller 能稳定输出 attach / detach / 失败路径

## Internal And External Dependencies

- 依赖 `prismtrace-core`
- 依赖 `prismtrace-storage`
- 真实 discovery 依赖系统 `ps` 命令

## Edit Hazards And Debugging Notes

- 这里容易膨胀成“所有东西都往 host 放”，后续加 attach / API / UI 之前要继续拆清模块边界
- 当前 `--discover` 只是最小本地入口，不要把它误认为最终产品 surface
- 当前 `--readiness` 也只是本地演示入口，不等于最终的目标选择体验
