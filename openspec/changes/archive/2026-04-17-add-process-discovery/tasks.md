## 1. 共享 discovery 模型

- [x] 1.1 在 `prismtrace-core` 中补充 host 所需的 discovery 进程样本与标准化类型
- [x] 1.2 为受控进程元数据输入下的 runtime 分类和显示名称标准化增加测试覆盖

## 2. Host discovery service

- [x] 2.1 在 host 侧新增一个 discovery 模块，将进程样本转换为结构化 `ProcessTarget`
- [x] 2.2 在 `prismtrace-host` 中增加一个返回 process target 集合的 discovery service 入口
- [x] 2.3 为 Node、Electron 和 Unknown 三类分类行为增加不依赖 live attach 的确定性测试

## 3. Host 集成与验证

- [x] 3.1 将 discovery service 接到一个可本地运行验证的最小 host 入口上
- [x] 3.2 验证加入 discovery 后，现有 host bootstrap 和本地状态目录初始化能力仍然通过
- [x] 3.3 如果本地开发入口在实现过程中发生变化，则同步更新 README 文档
