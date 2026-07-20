# AutoPause / AutoResume / AutoDelete 设计

## 范围

第一阶段复用现有 `Stop` / `Start`，不创建 Memory Snapshot，不增加 `Paused` 状态，也不承诺保留内存、进程、终端会话或网络连接。持久化磁盘和 Volume 继续由已有存储生命周期管理。

能力由云端控制面实现。REST backend 透传策略；嵌入式本地 backend 不运行清扫器，对显式的非默认生命周期配置返回 `BoxliteError::Unsupported`，避免静默忽略。

## 对外契约

公开 wire 字段统一为：

```text
auto_pause_interval:  integer seconds, 0 disables
auto_delete_interval: integer seconds, 0 disables
```

默认 AutoPause 为 `900` 秒，默认 AutoDelete 为 `0`。创建、读取、Rust runtime，以及 Python、Node.js、C、Go SDK 使用同一秒级语义。

内部数据库字段为 `autoPauseInterval`、`autoDeleteInterval`、`autoResumeEnabled` 和 `lastActivityAt`。公开名称保持稳定。

## 状态机

```text
STARTED -- idle deadline --> STOPPING --> STOPPED
   ^                                    |
   |------ user operation / Start ------|

STOPPED -- delete deadline --> DESTROYING --> DESTROYED
```

AutoPause 只选择以下 Box：

- `state = STARTED`
- `desiredState = STARTED`
- `pending = false`
- `autoPauseInterval > 0`
- 最后活动时间已超过秒级期限

AutoDelete 只选择以下 Box：

- `state = STOPPED`
- `desiredState = STOPPED`
- `pending = false`
- `autoDeleteInterval > 0`
- `lastActivityAt` 已超过秒级期限

`lastActivityAt` 是 AutoPause 和 AutoDelete 的共同计时标准。它在 Box 创建、状态变化或 Organization 变化时更新；因此 AutoDelete 从 Box 实际进入 `STOPPED`（或停止后最后一次被视作活动的事件）开始计时，而不是从发出 Stop 请求开始。

AutoDelete `0` 表示禁用；不再支持旧 `-1` 禁用语义或“停止时立即删除”语义。

## Activity policy

HTTP runner proxy 使用显式 policy，而不是共享入口的隐式默认值：

| 路径 | activity | autoResume |
|---|---:|---:|
| Exec 和 execution controls | true | true |
| Files | true | true |
| Metrics | false | false |

WebSocket attach 会触发 AutoResume，但 upgrade 本身不刷新 activity。只有代理建立后收到非空客户端数据帧才写入 activity。活动写入由 Redis lock TTL 节流，随后由周期任务批量刷新到数据库。

独立端口代理不写 activity，也不触发 AutoResume。这样 Metrics scrape、健康检查和外部端口流量不会使 Box 永久运行。

## Strict AutoResume gate

HTTP Exec/Files 和 WebSocket attach 共享 `BoxAutoResumeService`：

1. 解析 canonical Box ID 和 Organization。
2. activity操作先将时间写入Redis缓冲。
3. 使用与生命周期清扫器相同的逐Box状态锁。
4. `STARTED` 直接通过；`STOPPED` 通过条件更新提交 Start intent；正在 Start 的请求加入等待；正在 Stop 的请求先等待 `STOPPED`，再提交 Start。
5. 释放短临界区状态锁，不在冷启动期间持锁。
6. 通过Redis状态事件等待实际 `STARTED`，最长30秒。
7. 只有成功到达 `STARTED` 后才向runner转发。

waiter允许同一Box存在多个订阅者，并在订阅后重新读取状态，避免“第一次读取”和事件订阅之间丢事件。超时返回错误，不返回最后观察到的非目标状态。

分布式锁带随机owner token，并通过Redis Lua compare-and-delete释放；过期后旧worker不会误删新owner取得的锁。

## Sweeper与并发安全

AutoPause和AutoDelete每10秒运行一次，并各自使用全局worker锁。候选Box还要取得逐Box状态锁。

Activity先写Redis、后批量刷数据库，因此仅依赖SQL候选会产生最长一个flush周期的陈旧窗口。AutoPause在取得逐Box锁后通过 `BoxActivityService.getLastActivityAt` 重新读取Redis优先的最新时间；若最近有活动则跳过。

状态写入使用条件更新：

- AutoPause同时比较 `pending`、`state`、`desiredState` 和选中时的 `autoPauseInterval`；
- AutoDelete同时比较 `pending`、`state`、`desiredState` 和 `autoDeleteInterval`。

因此用户在候选查询后修改策略、手动Start/Stop，或另一个worker先提交状态变更时，旧候选不会覆盖新状态。

## 已删除的字段与端点

- 旧分钟字段 `autoStopInterval` 与 `autoDeleteInterval` 已删除。
- 旧秒级字段 `autoDeleteIntervalSeconds` 已重命名为 `autoDeleteInterval`。
- `POST /box/{boxIdOrName}/autostop/{interval}` 与 `POST /box/{boxIdOrName}/autodelete/{interval}` 端点已删除。

## Backend与SDK边界

`BoxLifecyclePolicy`在Rust核心层验证 sentinel和跨字段顺序。REST runtime：

- create将显式options放入REST body；
- Box response映射到 `BoxInfo`。

本地runtime：

- 未显式配置策略时维持现有行为；
- create/get_or_create收到任一生命周期option时返回 `Unsupported`。

C ABI使用 `uint32_t` AutoPause和 `uint32_t` AutoDelete，二者均以 `0` 表示禁用。Go bridge、Python和Node绑定使用对应的非负整数类型。

## 可观测性与故障语义

- AutoResume的数据库、锁、状态失败会向调用者传播；不能best-effort转发。
- suspended Organization沿用显式Start的403边界。
- WebSocket升级失败关闭socket；HTTP错误保留控制面状态码。
- Metrics和端口访问不会产生虚假activity。
- runner仍是实际Box状态的来源；控制面只写desired state，状态同步更新actual state和`lastActivityAt`。
