# Design: use-global-local-store

## Product Model

PrismTrace 的默认观测域是当前用户的本机会话集合，而不是某个 shell 启动目录。目录信息来自被观测应用的会话上下文，例如 Codex thread `cwd`，它应该是筛选和诊断字段。

## State Root

默认路径：

```text
~/Library/Application Support/PrismTrace
```

该路径下继续沿用已有 `StorageLayout`：

```text
state/
  artifacts/
  index/
  logs/
  tmp/
```

显式覆盖：

```bash
prismtrace --state-root /custom/prismtrace
```

也支持环境变量用于测试和自动化：

```bash
PRISMTRACE_STATE_ROOT=/custom/prismtrace prismtrace --console
```

优先级：

1. `--state-root`
2. `PRISMTRACE_STATE_ROOT`
3. macOS 用户级默认路径

## Legacy Compatibility

如果用户在某目录运行新版 PrismTrace，且当前目录存在旧版：

```text
./.prismtrace/state/artifacts/
```

host 会把旧 artifacts 复制到全局 store 中缺失的位置。原则：

- 只复制 artifacts，不复制旧 index 文件
- 不覆盖同名目标文件
- 复制失败不阻止启动，但启动摘要应可继续显示全局 state root

这样旧数据可通过新 read model 重建 index，避免把旧 cwd-scoped index 继续当成权威。

## Read Model Behavior

Codex rollout reader 默认不再传入 workspace/cwd 过滤条件。它应读取本机 Codex state DB 中所有未归档交互 thread，并用 thread/session 的 `cwd` 字段作为 metadata。

后续如果需要项目筛选，应通过 console/API filter 明确表达，而不是通过 state root 推断。

## CLI Shape

所有命令共享同一个 state root 解析逻辑：

```bash
prismtrace --console
prismtrace --opencode-observe
prismtrace --codex-observe
prismtrace --claude-observe
prismtrace --discover
```

高级覆盖：

```bash
prismtrace --console --state-root /tmp/prismtrace-test
```

## Risks

- 全局 store 会聚合更多本机敏感上下文，因此仍必须保持本地-only，不主动上传。
- 旧 artifact 复制可能重复导入少量 observer 事件；使用“目标不存在才复制”降低覆盖风险。
