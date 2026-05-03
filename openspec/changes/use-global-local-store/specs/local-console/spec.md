# local-console Delta

## Modified Requirements

### Requirement: Host 必须提供可本地访问的控制台入口
PrismTrace host MUST 提供一个可在本机访问的控制台入口，使用户无需直接查看 CLI dump 或手工打开 artifact 文件，也能进入本地可观测性界面。

#### Scenario: 启动 host 后可以打开本地控制台
- **WHEN** 用户启动带有控制台能力的 PrismTrace host
- **THEN** host 提供一个明确的本地访问入口，用户可以通过浏览器进入控制台
- **AND** 控制台默认读取用户级本机 PrismTrace state，而不是当前 shell 启动目录下的 state

#### Scenario: 控制台入口显示当前本机 state root
- **WHEN** 用户打开本地控制台或查看启动摘要
- **THEN** 系统暴露当前读取的 state root
- **AND** 没有会话时空态能够提示该 state root 尚无可展示 artifacts
