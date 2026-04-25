## 1. 文档与规格

- [x] 1.1 新增 `docs/stories/add-tool-visibility/design.md`
- [x] 1.2 新增 `docs/stories/add-tool-visibility/plan.md`
- [x] 1.3 新增 `openspec/changes/add-tool-visibility/proposal.md`
- [x] 1.4 新增 `openspec/changes/add-tool-visibility/design.md`
- [x] 1.5 新增 `openspec/changes/add-tool-visibility/tasks.md`
- [x] 1.6 新增 `openspec/changes/add-tool-visibility/specs/tool-visibility/spec.md`

## 2. host 侧采集

- [x] 2.1 新增 request-embedded tool visibility 提取与 artifact 落盘
- [x] 2.2 在 request capture 路径中接入 visibility 采集
- [x] 2.3 在前台 attach 消费循环中输出 visibility summary

## 3. console 侧展示

- [x] 3.1 扩展 request detail 读模型，增加 tool visibility detail
- [x] 3.2 扩展 request detail API payload
- [x] 3.3 扩展 request inspector UI，展示 tool visibility

## 4. 验证

- [x] 4.1 增加 tool visibility 提取 / 落盘测试
- [x] 4.2 增加 request inspector visibility 展示测试
- [x] 4.3 通过 `cargo fmt --check`
- [x] 4.4 通过 `cargo clippy --workspace --all-targets -- -D warnings`
- [x] 4.5 通过 `cargo test --workspace`
- [x] 4.6 通过 `cargo run -p prismtrace-host -- --discover`
