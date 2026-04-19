# 黑盒测试说明

## 测试目标

- 验证 PrismTrace host 已经从 CLI-first 的观测入口，推进到可以在浏览器中打开的本地控制台
- 验证控制台能够稳定展示 target、activity、request 摘要和基础 request 详情，而不是要求用户直接查看 artifacts
- 验证本地控制台的引入不会破坏现有 discovery、readiness、attach 与 request capture 路径

## 测试范围

- 覆盖 `prismtrace-host -- --console` 的启动与本地访问入口
- 覆盖控制台首页与 `/api/targets`、`/api/activity`、`/api/requests`、`/api/requests/:id` 的最小可用行为
- 覆盖 request 摘要读取、基础详情展示和空态/错误态语义

## 前置条件

- 在 macOS 本机运行 PrismTrace workspace
- 已具备 `process-discovery`、`attach-readiness`、`attach-controller`、`request-capture` 能力
- 可以执行 `cargo test` 与 `cargo run -p prismtrace-host`
- `.prismtrace/state` 下允许读取已有 request artifacts 作为控制台摘要数据源

## 操作约束

- 本次验证只关注“本地控制台最小闭环”，不验证完整 request inspector 或 response 深度渲染
- 控制台中的 request 详情只要求达到摘要和基础元数据可见，不要求等价于完整 payload 检查器
- 即使没有 active session 或没有已捕获 request，控制台仍需返回明确空态，而不是空白页

## 核心场景

### 1. 控制台入口可启动并给出本地地址

- 场景类型：成功
- 输入：执行 `cargo run -p prismtrace-host -- --console`
- 关注点：
  - host 输出明确的本地入口地址
  - 浏览器可访问主页
  - 启动失败时返回结构化错误而不是静默退出
- 预期：
  - 不应要求用户猜测访问地址
  - 不应退化为只有 CLI 日志、没有 Web 入口

### 2. 首页展示 target、activity、request 三个主区域

- 场景类型：成功
- 输入：打开控制台主页 `/`
- 关注点：
  - target 列表可见
  - 最近活动时间线可见
  - request 摘要列表可见
- 预期：
  - 不应出现空白页
  - 不应要求用户先手工查看 artifacts 才能理解数据

### 3. request 列表与基础详情可浏览

- 场景类型：成功
- 输入：系统中已存在至少一条 request artifact
- 关注点：
  - `/api/requests` 返回稳定结构
  - 控制台能展示 request provider、model、时间和 target 摘要
  - 进入单条 request 后能看到基础详情与 artifact 引用
- 预期：
  - 不应只停留在 request id 列表
  - 不应在详情缺失时无提示地失败

### 4. 没有数据时返回明确空态

- 场景类型：空态
- 输入：当前没有发现 target、没有 active session 或没有已捕获 request
- 关注点：
  - target/activity/request 都有明确空态说明
  - health 区域不会误导用户认为系统正常观测中
- 预期：
  - 不应显示模糊空白区域
  - 不应把“没有数据”误报成“请求失败”

### 5. 引入控制台后不破坏现有 CLI 入口

- 场景类型：回归
- 输入：继续运行 `--discover`、`--readiness`、`--attach` 等既有路径
- 关注点：
  - 控制台新增入口不会破坏现有 bootstrap
  - 原有 CLI 路径仍能正常执行
- 预期：
  - 不应因为引入 HTTP/UI surface 而让现有能力回退

## 通过标准

- `--console` 可以输出明确的本地访问入口
- 控制台主页可稳定展示 target、activity、request 三个主区域
- request 摘要列表与基础详情都能返回稳定结构
- 无数据时有明确空态说明
- 引入控制台后，既有 CLI 能力未回退

## 回归重点

- `process-discovery` 与 `attach-readiness` 输出是否仍然稳定
- attach / request capture 相关测试是否仍然通过
- host bootstrap 与 `.prismtrace/state` 初始化是否保持不变

## 自动化验证对应

- `crates/prismtrace-host/src/console.rs`
  - 覆盖控制台快照聚合、HTTP 路由、JSON payload 与静态页面渲染
- `crates/prismtrace-host/src/main.rs`
  - 覆盖 `--console` 参数入口
- `crates/prismtrace-host/src/lib.rs`
  - 覆盖控制台与 bootstrap 的集成、启动摘要与 URL 输出

## 测试环境待补充项

- 真正的浏览器端交互与更复杂的 request inspector 体验，将在后续 change 中继续补黑盒验证
- 带过滤上下文的控制台浏览行为，将在后续 `add-console-target-filter` 变更中补充验证
