<!-- PRAXIS_DEVOS_START -->
> This block is maintained by Praxis DevOS. Run `npx praxis-devos@latest update` to refresh it.

## Flow Selection

- 根据请求性质选择 OpenSpec flow：

### 使用 explore / propose

当请求具备以下任一特征时，必须进入 OpenSpec（从 explore 或 propose 开始）：

- 中大型改动
- 跨模块 / 跨系统变更
- 涉及接口、兼容性或架构调整
- 存在不明确需求或未收敛的 open questions
- 存在多个可选方案需要对比或决策
- 引入新能力或新 workflow

典型示例：

- “帮我加一个 X”
- “新增 Y 能力”
- “我想做一套 Z workflow”
- “implement feature X”
- “add a release kit”

要求：

- 在 explore / propose 阶段完成需求澄清与方案收敛后，才能进入实现

---

### 使用 apply（直接实现）

仅当请求满足以下条件时，可以直接进入实现阶段：

- 改动范围小且局部
- 无设计歧义
- 不涉及架构或接口变化
- 不需要方案对比或前置设计

典型示例：

- “修一下这个 bug”
- “改一下这段文案”
- “update the version number”
- “fix the failing test”

---

### 使用 review flow

- 评审、审计、分析类请求应使用 review flow

---

## OpenSpec + SuperPowers Contract（简化）

- OpenSpec（explore / propose / apply / archive）是唯一对用户可见的流程层
- SuperPowers 仅作为阶段内嵌能力使用，不形成独立流程
- 所有产物必须收敛在当前 change 下，不得创建额外目录（如 `docs/superpowers/...`）
<!-- PRAXIS_DEVOS_END -->

# PrismTrace

PrismTrace（棱镜观测）是一个 macOS 上的 AI 应用可观测性工具，目标是在不重启目标应用的前提下，观测运行中的 Node / Electron AI 应用真实发给模型的内容。当前仍处于 V1 bootstrap 阶段，真实动态注入后端尚未完成。

## 输出约定

- 默认用中文写方案、计划、评审结论、实现说明和进度同步。
- 代码、命令、标识符、提交信息可以保留英文。

## 先看这几个地方

- `docs/总体设计与V1方案.md`
- `docs/产品迭代路线图.md`
- `openspec/`

## 工作方式

- 在进入实现前，先结合现有设计文档、路线图理解目标与边界。
- 做中大型改动、跨模块改动、接口或架构调整时，先做设计收敛，再进入实现。
- 如果需求已经绑定到某个 story 或 change，优先复用已有文档与目录，不重复新建平行方案。
- 如无明确要求，不主动扩散范围，不顺手做无关重构。
- 对纯 CSS、样式调整、静态前端页面改版这类工作，强制禁止走 TDD；禁止为这类改动编写单元测试代码，也禁止为了验证这类改动主动跑测试。
- 推代码前必须先通过本地 CI 基线，至少覆盖 `cargo fmt --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` 和 `cargo run -p prismtrace-host -- --discover`。

## Story 文档组织

- 针对具体 story 的设计稿与实现计划，放在同一个目录下，避免分散到不同位置。
- 默认路径约定：

```text
docs/stories/<story-slug>/design.md
docs/stories/<story-slug>/plan.md
```
