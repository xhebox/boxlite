# AutoPause、AutoResume 与 AutoDelete

BoxLite 云端 REST runtime 可以在 Box 空闲后自动停止虚拟机，并在下一次用户操作到来时重新启动。该能力复用已有的 `Stop` / `Start` 生命周期，不创建内存快照，也不引入新的 `Paused` 状态。

> 本文描述云端 REST runtime。嵌入式本地 runtime 不执行生命周期清扫；显式配置这些策略会返回 `Unsupported`。

## 配置

生命周期间隔统一使用秒：

| 字段 | 默认值 | 禁用值 | 含义 |
|---|---:|---:|---|
| `auto_pause_interval` | `900` | `0` | 最后一次有效活动后等待多久执行 Stop |
| `auto_delete_interval` | `0` | `0` | Box 成功停止后等待多久删除 |

创建 Box 时可以设置策略：

```json
{
  "image": "python:3.13",
  "auto_pause_interval": 900,
  "auto_delete_interval": 604800
}
```

设置 `auto_pause_interval: 0` 会关闭 AutoPause；设置 `auto_delete_interval: 0` 会关闭 AutoDelete。同时启用时，AutoDelete 间隔必须大于 AutoPause 间隔。

Python、Node.js、C 和 Go SDK 均可在创建时传入这两个字段。Box info 会返回当前生效的秒级值。

## 生命周期行为

AutoPause 的行为是：

1. Box 处于 `STARTED`，并且没有正在进行的状态变更。
2. 最后一次有效活动超过 `auto_pause_interval`。
3. 控制面提交 `STOPPED` 目标状态，并走正常 Stop 流程。
4. 虚拟机停止后，Box 进入已有的 `STOPPED` 状态。

AutoResume 的行为是：

1. 用户对已停止的 Box 发起 Exec、Files 或 WebSocket attach 操作。
2. 控制面提交或加入已有的 Start 操作。
3. 首个请求等待 Box 真正到达 `STARTED` 后再转发到 runner。
4. 启动失败或超时时，请求直接失败，不会提前转发到尚未就绪的 Box。

首次请求会承担冷启动延迟。多个并发请求会共享同一个 Start 状态变更，并分别等待同一状态事件。

AutoDelete 从 Box 成功进入 `STOPPED` 时开始计时。到期后 Box 被删除，之后不能再通过 AutoResume 恢复。手动 Stop 也会开始该计时；将策略改为 `0` 会取消后续自动删除。

## 哪些操作算作活动

| 操作 | 刷新活动时间 | 触发 AutoResume |
|---|---|---|
| Exec，以及 execution status/signal/resize/kill | 是 | 是 |
| Files 读写 | 是 | 是 |
| WebSocket attach | 仅收到真实客户端数据帧时 | 是 |
| Metrics | 否 | 否 |
| 端口预览和端口代理 | 否 | 否 |

Metrics 和端口流量属于观察或外部服务流量。持续抓取指标、健康检查或访问暴露端口不会让 Box 永久保持运行，也不会自动启动已停止的 Box。

## Stop 后保留什么

AutoPause 不保存运行时内存。Stop 后：

- 持久化磁盘和挂载的 Volume 会保留；
- 内存、进程和后台任务不会保留；
- 终端会话和网络连接会断开；
- AutoResume 后由镜像、持久化磁盘和应用启动逻辑重新建立运行环境。

需要跨 Stop 保留的数据必须写入持久化磁盘或 Volume。不要依赖内存变量、后台进程或未落盘的文件。

## 计费模型

AutoPause 的目的，是在空闲时停止计算资源。计费仍沿用平台现有的计量维度：

- CPU
- RAM
- GPU
- Disk

运行中的计算资源与停止后保留的持久化存储属于不同维度。具体单价、免费额度和结算规则以部署环境的计费页面与商业条款为准；本文不承诺固定价格。

## 常见问题

### 为什么访问 Metrics 没有自动启动 Box？

这是预期行为。Metrics 不属于用户工作负载活动，否则监控系统会阻止 AutoPause。

### 为什么端口服务停止了？

端口代理不计活动。若服务需要常驻运行，请关闭 AutoPause，或通过真实的 Exec / Files / attach 工作流管理生命周期。

### AutoResume 会恢复原来的 shell 或进程吗？

不会。AutoResume 是 Start，不是内存恢复。应用需要具备正常的重启能力。

### 关闭 AutoPause 后还能使用 AutoDelete 吗？

可以。此时 AutoDelete 不会主动停止运行中的 Box，但 Box 手动停止后仍会根据 `auto_delete_interval` 删除。
