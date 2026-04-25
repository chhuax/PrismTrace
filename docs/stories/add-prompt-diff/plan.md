# add-prompt-diff 实现计划

> 给执行代理的要求：本计划只推进 prompt diff 第一版，不并行扩展 tool visibility diff、failure attribution 或 skill diagnostics。以下步骤使用 `- [ ]` 语法追踪。

**目标：** 把 PrismTrace 从“能看见一条 request 的事实”推进到“能比较同一 session 内相邻两次 request 的 prompt 变化”，让 request inspector 能直接回答“这次 prompt 相比上一条变了什么”。

**架构：** 继续以现有 request artifact、session reconstruction 和 local console 为基础，在 host 侧增加 prompt projection / diff 读模型，并把结果接到 request inspector。优先复用现有 artifacts 和 session 顺序，不引入新的存储通道。

**技术栈：** Rust workspace、本地 host 控制台、OpenSpec 文档驱动开发。

---

## 1. 收敛 prompt diff V1 边界

- [ ] 1.1 固定 diff 比较对象
  - 验证：文档明确只比较同一 session 内相邻两次 request

- [ ] 1.2 固定 projection 范围
  - 验证：文档明确只提取 prompt-bearing 文本，不扩展到 tools / 参数 diff

- [ ] 1.3 固定不可比较状态
  - 验证：文档明确 `available`、`no_previous_request`、`unavailable_projection` 三类状态

## 2. 实现 host 侧 prompt projection / diff 读模型

- [ ] 2.1 增加 request body 到 prompt projection 的提取逻辑
  - 验证：常见 `system` / `instructions` / `messages` / `input` 路径可稳定投影为文本

- [ ] 2.2 增加相邻 request prompt diff 逻辑
  - 验证：同一 session 内当前 request 可关联上一条 request 并生成 diff

- [ ] 2.3 扩展 request detail 返回 prompt diff
  - 验证：request inspector API 能返回 diff 状态、上一条 request 引用和 diff 文本

## 3. 扩展控制台 request inspector

- [ ] 3.1 新增 `Prompt Diff` 区域
  - 验证：detail UI 可展示上一条 request 引用、diff 状态和 diff 文本

- [ ] 3.2 处理空态与不可比较态
  - 验证：没有上一条 request 或无法提取 projection 时，UI 有明确说明

## 4. 验证与收尾

- [ ] 4.1 增加聚焦测试
  - 验证：覆盖 projection 提取、相邻 request diff、无上一条 request 和不可提取 projection 的降级路径

- [ ] 4.2 运行本地 CI 基线
  - 验证：通过 `cargo fmt --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace`、`cargo run -p prismtrace-host -- --discover`
