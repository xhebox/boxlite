# BoxLite、E2B、CubeSandbox 与 Firecracker：Snapshot 和 Pause/Resume 调研

> 调研日期：2026-07-14  
> Firecracker：[`1faca2f70e7af6672fc7b4daacd9d037c874c335`](https://github.com/firecracker-microvm/firecracker/commit/1faca2f70e7af6672fc7b4daacd9d037c874c335)  
> E2B Infra：[`5b465bc3e72aaca4a8c9dd80fada0c3854cd4431`](https://github.com/e2b-dev/infra/commit/5b465bc3e72aaca4a8c9dd80fada0c3854cd4431)  
> CubeSandbox：[`3a1274d68d7778b62dccfc0056b212ee5fb750fa`](https://github.com/TencentCloud/CubeSandbox/commit/3a1274d68d7778b62dccfc0056b212ee5fb750fa)，最新正式版本 [`v0.5.1`](https://github.com/TencentCloud/CubeSandbox/releases/tag/v0.5.1)  
> BoxLite：主线及最新正式版本 [`v0.9.7`](https://github.com/boxlite-ai/boxlite/releases/tag/v0.9.7)

## 1. 结论

四者的 `Snapshot` 和 `Pause` 不是同一种能力：

| 能力 | Firecracker | E2B | CubeSandbox | BoxLite 当前主线 |
| --- | --- | --- | --- | --- |
| 磁盘 Snapshot | 不负责，由上层保存 backing files | 保存 rootfs dirty-block layer | XFS reflink/CoW 保存 rootfs；Pause 保留当前 writable rootfs | 支持 QCOW2 external COW |
| Memory Snapshot | 支持 Full/Diff RAM | 支持分层、去重的 RAM diff | 支持 Full、匿名页 Incremental、SoftDirty | 不支持 |
| VM state Snapshot | 支持 vCPU、KVM VM、设备状态 | 使用自定义 Firecracker 保存 | 基于定制 Cloud Hypervisor 保存 vCPU、VM、设备状态 | 不支持 |
| Pause/Resume | 暂停/恢复同一个 Firecracker VM | Pause 后持久化并销毁旧 VM；Resume 创建新 VM | Pause 后持久化并删除 MicroVM 对象；Resume 从 Snapshot 重建 | 内部用 `SIGSTOP`/`SIGCONT` 短时冻结；公开 API 尚未合并 |
| 恢复原执行位置 | 支持 | Full-memory 模式支持 | 支持 | 不支持，恢复磁盘后冷启动 |
| Lazy memory restore | `MAP_PRIVATE` 或 UFFD | UFFD + cache + remote storage + prefetch | `MAP_PRIVATE` 文件映射，可选 prefault | 不支持 |
| Auto-pause | 不提供平台策略 | timeout 到期触发 | timeout 到期触发；**仅入站请求刷新活跃时间** | 不支持 |
| Auto-resume | 不提供平台策略 | **仅支持 Full-memory Snapshot**；由流量访问触发，filesystem-only 禁止 | 入站请求触发完整 VM Snapshot 恢复；也可显式恢复 | 不支持 |

## 2. 先区分四个概念

| 概念 | 保存内容 | 原 VM 是否存活 | 恢复语义 |
| --- | --- | --- | --- |
| 进程冻结 | 活进程中的全部状态 | 是，且继续占 RAM | 解冻同一进程 |
| 磁盘 Snapshot | writable disk/COW layer | 不要求 | 新 VM 从旧磁盘状态冷启动 |
| Memory/VM Snapshot | RAM + vCPU + VM/device state | 可退出 | 新 VMM 恢复原执行位置 |
| 平台 Pause | 由产品定义 | 取决于实现 | E2B/CubeSandbox 指持久化后释放 Guest VM；BoxLite 内部指冻结原进程 |

要从 Snapshot 时的指令位置继续执行，至少需要同时恢复：

```text
Guest RAM
vCPU registers / architecture state
KVM VM state
virtual device and virtqueue state
匹配时间点的磁盘状态
```

只复制 RAM 或磁盘都不够。

## 3. Firecracker：Memory Snapshot 的底层机制

### 3.1 制品与一致性边界

上游 Firecracker 将 VM 状态拆成：

- `vmstate`：内存布局、KVM、vCPU、Virtio/MMIO/PCI/ACPI 等设备状态；
- `memfile`：Guest physical memory；
- 磁盘 backing files：不包含在 Snapshot API 中，调用方必须单独保存。

`vmstate` 中的核心结构是 `MicrovmState`：

```rust
pub struct MicrovmState {
    pub vm_info: VmInfo,
    pub kvm_state: KvmState,
    pub vm_state: VmState,
    pub vcpu_states: Vec<VcpuState>,
    pub device_states: DevicesState,
}
```

当前文件带有架构相关 magic、Snapshot format version、bitcode 序列化状态和 CRC64。CRC 用于发现意外损坏，不是密码学认证。

源码入口：

- [`MicrovmState` 和 create/restore orchestration](https://github.com/firecracker-microvm/firecracker/blob/1faca2f70e7af6672fc7b4daacd9d037c874c335/src/vmm/src/persist.rs)
- [`Vmm::save_state`](https://github.com/firecracker-microvm/firecracker/blob/1faca2f70e7af6672fc7b4daacd9d037c874c335/src/vmm/src/lib.rs#L500)
- [Snapshot file format](https://github.com/firecracker-microvm/firecracker/blob/1faca2f70e7af6672fc7b4daacd9d037c874c335/src/vmm/src/snapshot/mod.rs)

### 3.2 创建流程

调用方必须先将 microVM Pause，再调用 `/snapshot/create`：

```text
Running microVM
    ↓ PATCH /vm → Paused
所有 vCPU 退出 KVM_RUN，进入 Paused state
    ↓
保存 device state
    ↓
保存 vCPU 和 KVM VM state
    ↓
序列化 MicrovmState → vmstate
    ↓
Full：写全部 Guest RAM
Diff：写 dirty/resident pages
    ↓
生成 vmstate + memfile
```

Pause 是 stop-the-world 一致性点。否则 CPU register、设备队列、中断状态和 RAM 可能来自不同时间点。它暂停的是 Firecracker 管理的 VM/vCPU，而不是通过 `SIGSTOP` 冻结整个 host process。

保存设备状态必须早于 KVM 状态，因为设备的 `prepare_save()` 可能 drain I/O、更新 transport 或注入中断；先保存 KVM 会漏掉这些变化。

源码入口：

- [`Vmm::pause_vm`](https://github.com/firecracker-microvm/firecracker/blob/1faca2f70e7af6672fc7b4daacd9d037c874c335/src/vmm/src/lib.rs#L475)
- [vCPU Running → Paused](https://github.com/firecracker-microvm/firecracker/blob/1faca2f70e7af6672fc7b4daacd9d037c874c335/src/vmm/src/vstate/vcpu.rs#L254)
- [`create_snapshot`](https://github.com/firecracker-microvm/firecracker/blob/1faca2f70e7af6672fc7b4daacd9d037c874c335/src/vmm/src/persist.rs#L165)
- [Device persistence](https://github.com/firecracker-microvm/firecracker/blob/1faca2f70e7af6672fc7b4daacd9d037c874c335/src/vmm/src/device_manager/persist.rs)

### 3.3 Full 与 Diff RAM

Full Snapshot 同步遍历 Guest memory regions，将全部 plugged RAM 写入 memfile。创建成本随 RAM 大小增长，也可能 fault-in 原本不 resident 的页面。

Diff Snapshot 只在对应 offset 写选中的页面，其余区域用 seek 留作 sparse holes：

```text
logical file size ≈ Guest RAM size
allocated blocks ≈ dirty/resident pages
```

页面来源有两条路径：

1. 开启 `track_dirty_pages`：合并 KVM dirty log 与 Firecracker userspace dirty bitmap。后者用于覆盖设备模拟器直接写 Guest memory、KVM 看不到的页面。
2. 未开启 dirty tracking：使用 `mincore()` 把 resident pages 当作近似集合。它会多保存页面，而且 swap 开启时不能可靠覆盖被换出的脏页。

Snapshot 成功后会清理 dirty state，并重新标记必要的 Virtio queue memory。Diff 通常需要按代际覆盖到 Full Base 后才能恢复；sparse hole 的语义是“沿用 Base”，不是零页。

源码入口：

- [`snapshot_memory_to_file`](https://github.com/firecracker-microvm/firecracker/blob/1faca2f70e7af6672fc7b4daacd9d037c874c335/src/vmm/src/vstate/vm.rs#L566)
- [`dump` / `dump_dirty`](https://github.com/firecracker-microvm/firecracker/blob/1faca2f70e7af6672fc7b4daacd9d037c874c335/src/vmm/src/vstate/memory.rs)
- [`mincore_bitmap`](https://github.com/firecracker-microvm/firecracker/blob/1faca2f70e7af6672fc7b4daacd9d037c874c335/src/vmm/src/vstate/vm.rs#L729)
- [Diff Snapshot 和 rebase](https://github.com/firecracker-microvm/firecracker/blob/1faca2f70e7af6672fc7b4daacd9d037c874c335/docs/snapshotting/snapshot-support.md#creating-diff-snapshots)

### 3.4 恢复与 Lazy Loading

Load 发生在新 Firecracker process 的 pre-boot 阶段：

```text
读取并校验 vmstate
    ↓
建立 Guest memory backend
    ↓
创建新的 KVM VM 和 vCPU
    ↓
恢复 VM、vCPU 和设备状态
    ↓
初始保持 Paused
    ↓ PATCH /vm → Resumed
Guest 从原执行位置继续
```

Host 侧得到的是新 process、新 KVM VM fd、新 vCPU threads 和新设备对象；只有 Guest 观察到执行现场被延续。

两种内存 backend：

- **File**：对 memfile 建立 `MAP_PRIVATE` 映射。页面由 host kernel 按需 fault-in；Guest 写入触发匿名 COW，不修改原 memfile。多个 VM 可共享 clean page cache。
- **UFFD**：匿名映射 Guest RAM，注册 `userfaultfd`，把 UFFD fd 和 mappings 交给外部 handler。handler 可从本地缓存、远端、压缩层或 Snapshot chain 加载页面。

因此 Restore 快不代表 RAM 已全部加载，而是把大部分 I/O 推迟到 Guest 访问页面时。UFFD handler 若失效，VM 可能卡在 page fault，平台必须负责监控和回收。

源码入口：

- [`restore_from_snapshot`](https://github.com/firecracker-microvm/firecracker/blob/1faca2f70e7af6672fc7b4daacd9d037c874c335/src/vmm/src/persist.rs#L361)
- [File backend `MAP_PRIVATE`](https://github.com/firecracker-microvm/firecracker/blob/1faca2f70e7af6672fc7b4daacd9d037c874c335/src/vmm/src/vstate/memory.rs#L604)
- [`guest_memory_from_uffd`](https://github.com/firecracker-microvm/firecracker/blob/1faca2f70e7af6672fc7b4daacd9d037c874c335/src/vmm/src/persist.rs#L531)
- [`build_microvm_from_snapshot`](https://github.com/firecracker-microvm/firecracker/blob/1faca2f70e7af6672fc7b4daacd9d037c874c335/src/vmm/src/builder.rs#L425)

### 3.5 Firecracker 不解决的平台问题

Firecracker 只提供 Snapshot 原语，不负责：

- 磁盘 COW、应用 flush/fsfreeze 及 RAM 与磁盘的原子对应；
- Snapshot 分层、压缩、加密、对象存储和生命周期；
- 跨节点调度及 TAP、vsock、block path 的重建；
- Guest clock、外部 TCP 连接、应用身份与密钥的完全透明恢复。

兼容性至少涉及 CPU 架构/vendor/features、CPU template、Firecracker Snapshot format、KVM/host kernel 和外部设备资源。相同 Firecracker 版本也不普遍保证跨 host kernel 恢复。多个 VM 从同一 Snapshot 克隆还会复制 Guest 内存中的随机状态、token 和应用身份，需要额外轮换。

参考：

- [Snapshot support and limitations](https://github.com/firecracker-microvm/firecracker/blob/1faca2f70e7af6672fc7b4daacd9d037c874c335/docs/snapshotting/snapshot-support.md)
- [Versioning and compatibility](https://github.com/firecracker-microvm/firecracker/blob/1faca2f70e7af6672fc7b4daacd9d037c874c335/docs/snapshotting/versioning.md)
- [UFFD integration](https://github.com/firecracker-microvm/firecracker/blob/1faca2f70e7af6672fc7b4daacd9d037c874c335/docs/snapshotting/handling-page-faults-on-snapshot-resume.md)

## 4. E2B：把 Firecracker 原语做成平台 Pause/Resume

### 4.1 Pause 和 Resume

E2B 默认 Pause 保存：

- `snapfile`：Firecracker VM/vCPU/device state；
- `memfile`：Guest RAM 的页面增量；
- `rootfs`：可写文件系统 dirty-block 增量；
- metadata 和 memfile/rootfs headers：版本、索引和依赖链。

流程是：

```text
Running
  ↓ 停止 health checks，best-effort reclaim/sync
Firecracker VM Paused
  ↓ 保存 snapfile
提取 memory pages + rootfs dirty blocks
  ↓ 建立 diff/header chain，写本地 cache
后台上传对象存储
  ↓
终止旧 Firecracker process
```

Pause 返回时本地制品已可用，远端上传可能仍在后台继续。旧 VM 随后被销毁，因此释放 CPU 和大部分 RAM；它不是长期冻结原进程。

Resume 会优先选择 origin node，利用 local cache；否则从 P2P/对象存储取得制品，然后创建新的 rootfs overlay、Firecracker process 和 UFFD handler。Firecracker 加载 `snapfile` 后恢复 vCPU，内存页在 fault 时按需加载，并可按记录的热页关系 prefetch。

完整内存模式可以恢复 Guest kernel、进程、线程、内存变量和执行位置，但外部 TCP/NAT/conntrack 等资源仍可能失效。

源码入口：

- [Pause implementation](https://github.com/e2b-dev/infra/blob/5b465bc3e72aaca4a8c9dd80fada0c3854cd4431/packages/orchestrator/pkg/sandbox/sandbox.go#L1251)
- [Resume implementation](https://github.com/e2b-dev/infra/blob/5b465bc3e72aaca4a8c9dd80fada0c3854cd4431/packages/orchestrator/pkg/sandbox/sandbox.go#L710)
- [Snapshot data model](https://github.com/e2b-dev/infra/blob/5b465bc3e72aaca4a8c9dd80fada0c3854cd4431/packages/orchestrator/pkg/sandbox/snapshot.go#L30)
- [UFFD handler](https://github.com/e2b-dev/infra/blob/5b465bc3e72aaca4a8c9dd80fada0c3854cd4431/packages/orchestrator/pkg/sandbox/uffd/uffd.go)

### 4.2 E2B 对 Firecracker 的扩展

E2B 的内存层不是上游 Firecracker 原生 sparse memfile 的简单搬运。其自定义 Firecracker 暴露 memory info、dirty bitmap 和 host mappings；Orchestrator 再从 memfd 或通过 `process_vm_readv` 读取选定页面，进行分层、去重、压缩和持久化。

```text
自定义 Firecracker
  ├─ resident / empty pages
  ├─ dirty bitmap
  └─ Guest offset → host virtual address mappings
             ↓
Orchestrator 提取页面
             ↓
与 parent layer 比较、去重
             ↓
local cache / P2P / object storage
             ↓
UFFD handler 分层按页恢复
```

首次 Snapshot 主要根据 resident/empty metadata 选择页面；后续代际结合 UFFD page state 和 dirty bitmap。E2B 还允许 `mem_file_path` 缺省，使 Firecracker 只输出 VM state，而 RAM 由 Orchestrator 自行构建。

源码入口：

- [Firecracker memory API client](https://github.com/e2b-dev/infra/blob/5b465bc3e72aaca4a8c9dd80fada0c3854cd4431/packages/orchestrator/pkg/sandbox/fc/client.go#L500)
- [Memory metadata](https://github.com/e2b-dev/infra/blob/5b465bc3e72aaca4a8c9dd80fada0c3854cd4431/packages/orchestrator/pkg/sandbox/fc/memory.go)
- [`process_vm_readv` extraction](https://github.com/e2b-dev/infra/blob/5b465bc3e72aaca4a8c9dd80fada0c3854cd4431/packages/orchestrator/pkg/sandbox/block/cache.go#L647)
- [Memory prefetcher](https://github.com/e2b-dev/infra/blob/5b465bc3e72aaca4a8c9dd80fada0c3854cd4431/packages/orchestrator/pkg/sandbox/uffd/prefetch/prefetcher.go)

### 4.3 Full-memory、filesystem-only 与 Checkpoint

| 操作 | 保存内容 | 操作后状态 | 显式恢复 | 流量 Auto-resume |
| --- | --- | --- | --- | --- |
| Pause（默认） | VM + RAM + rootfs | 停止 | 恢复执行现场 | 支持 |
| Pause（`memory: false`） | rootfs | 停止 | 冷启动，丢失进程状态 | **不支持** |
| Snapshot/Checkpoint | VM + RAM + rootfs | 立即恢复 Running | 不适用 | 不适用 |

Filesystem-only 模式虽会调用 Firecracker Snapshot 以 drain/flush Virtio disk，但不持久化可恢复的 `snapfile` 和 `memfile`。它可以通过显式 `connect`/`resume` 恢复，但实际走 `RebootSandbox` 冷启动；**不能通过流量 Auto-resume**。普通 Memory Snapshot 也不能随意降级为 filesystem-only cold boot，因为部分已确认的写入可能仍只存在于 Snapshot 中的 Guest page cache。

公开 Snapshot API 实际执行 Checkpoint：Pause、生成 Full-memory template，然后立即从该 Snapshot 创建新 Firecracker lifecycle；Sandbox ID 保持不变，但并不是同一 Firecracker 进程上的 Pause/Resume。

源码入口：

- [Filesystem-only option](https://github.com/e2b-dev/infra/blob/5b465bc3e72aaca4a8c9dd80fada0c3854cd4431/packages/orchestrator/pkg/sandbox/sandbox.go#L1243)
- [Filesystem-only reboot](https://github.com/e2b-dev/infra/blob/5b465bc3e72aaca4a8c9dd80fada0c3854cd4431/packages/orchestrator/pkg/sandbox/reboot.go#L34)
- [Checkpoint implementation](https://github.com/e2b-dev/infra/blob/5b465bc3e72aaca4a8c9dd80fada0c3854cd4431/packages/orchestrator/pkg/server/sandboxes.go#L706)

### 4.4 Auto-pause 与 Auto-resume

两项能力默认都关闭。

Auto-pause 不是 CPU、进程或网络 idle detector，而是 timeout policy：

```text
Sandbox EndTime 到期
  ↓ Evictor 找到过期记录
AutoPause=false → Kill
AutoPause=true  → Pause + Snapshot
```

默认保存内存；`autoPauseMemory=false` 时只保存 rootfs，下次显式 Resume 是冷启动。

**关键限制：E2B 的流量 Auto-resume 只支持 Full-memory Snapshot，不支持 filesystem-only（非 Memory）Snapshot。**

这不是“filesystem-only 完全不能恢复”：

- 显式调用 `connect`/`resume`：支持，Orchestrator 走 `RebootSandbox`，从 rootfs 冷启动；
- 入站流量触发 Auto-resume：拒绝，因为它会隐式丢失原 Guest RAM、进程、线程和 socket 状态。

E2B 在两处强制该约束：

1. 创建 Sandbox 时，`autoPauseMemory=false` 不能与 `autoResume` 同时配置；
2. 流量恢复时再次检查 Snapshot，filesystem-only 会返回 `filesystem-only snapshot must be resumed explicitly`。

Full-memory Auto-resume 的流程为：

```text
请求到达 client-proxy
  ↓ running catalog miss
检查 Snapshot、权限和 autoResume policy
  ↓ 仅接受 Full-memory Snapshot
调度并 Resume Sandbox
  ↓
Sandbox ready 后转发原请求
```

首个请求承担 Resume 延迟。调度还要满足 CPU、Firecracker、kernel、Snapshot format 和 envd 等兼容条件。

源码入口：

- [Auto-pause Evictor](https://github.com/e2b-dev/infra/blob/5b465bc3e72aaca4a8c9dd80fada0c3854cd4431/packages/api/internal/orchestrator/evictor/evict.go#L151)
- [Filesystem-only 与 Auto-resume 配置互斥](https://github.com/e2b-dev/infra/blob/5b465bc3e72aaca4a8c9dd80fada0c3854cd4431/packages/api/internal/handlers/sandbox_create.go#L170)
- [Traffic Auto-resume 的 filesystem-only 拒绝逻辑](https://github.com/e2b-dev/infra/blob/5b465bc3e72aaca4a8c9dd80fada0c3854cd4431/packages/api/internal/handlers/proxy_grpc.go#L117)
- [显式 Resume 的 reboot/resume 分流](https://github.com/e2b-dev/infra/blob/5b465bc3e72aaca4a8c9dd80fada0c3854cd4431/packages/orchestrator/pkg/server/sandboxes.go#L220)

## 5. CubeSandbox：完整 VM Snapshot 上的平台 Auto-pause/Auto-resume

### 5.1 Pause/Resume 与 Snapshot 机制

CubeSandbox 使用定制 Cloud Hypervisor，而不是 Firecracker。其 Pause 不是 `SIGSTOP` 冻结，也不是磁盘冷启动：

```text
Running MicroVM
  ↓ CubeShim 断开 Guest agent，调用 VmPauseToSnapshot
暂停 vCPU，保存 VM/vCPU/device state
  ↓ 默认 Full 模式写 Guest RAM
保存 Snapshot 到 /data/cubelet/root/pausevm/<sandbox>
  ↓ 删除当前 MicroVM 对象
Paused：Guest CPU/RAM 已释放，当前 writable rootfs 保留
  ↓ ResumeFromSnapshot
从 VM state + RAM + rootfs 恢复并重新连接 Guest agent
Running，回到原执行现场
```

Hypervisor 的 `vm_pause_to_snapshot()` 顺序是 `vm_pause()` → `vm_snapshot()` → `vm_delete()`；恢复则读取 VM config/state 和内存镜像，创建新的 VM 对象、恢复设备与 vCPU 状态后运行。CubeShim/VMM 进程可以继续存在，但旧 MicroVM 对象和 Guest RAM 已删除，因此这里的“恢复”不是继续运行同一个活 VM 对象。

普通应用 Snapshot 会在短暂停顿中执行 pause → snapshot → resume，源 Sandbox 继续运行。内存支持三种写出模式：

| 模式 | 写入内容 | 约束 |
| --- | --- | --- |
| Full | 全部 Guest RAM | 自包含；Pause 路径当前使用默认 Full |
| Incremental | `present ∧ anonymous` 的 CoW 匿名页 | 目标必须已有由上一份内存镜像 reflink 得到的基线 |
| SoftDirty | `present ∧ anonymous ∧ soft_dirty` | 依赖 `CONFIG_MEM_SOFT_DIRTY`；不支持时降级为 Incremental |

恢复把内存镜像以 `MAP_PRIVATE` 映射为 Guest RAM，未访问页面由 Linux page fault 按需载入；`prefault` 可选择预取。它没有 E2B 的 UFFD 分层远端 page server、去重 cache 和热页 prefetch 管线。文件系统 Snapshot/clone 使用 XFS reflink CoW；Pause 则保持同一 Sandbox 的当前 writable rootfs，与同一时点的 VM/RAM state 配合恢复。

源码入口：

- [CubeShim Pause/Resume orchestration](https://github.com/TencentCloud/CubeSandbox/blob/3a1274d68d7778b62dccfc0056b212ee5fb750fa/CubeShim/shim/src/sandbox/sb.rs#L1194)
- [Cloud Hypervisor PauseToSnapshot](https://github.com/TencentCloud/CubeSandbox/blob/3a1274d68d7778b62dccfc0056b212ee5fb750fa/hypervisor/vmm/src/lib.rs#L616)
- [Cloud Hypervisor restore](https://github.com/TencentCloud/CubeSandbox/blob/3a1274d68d7778b62dccfc0056b212ee5fb750fa/hypervisor/vmm/src/lib.rs#L633)
- [Memory Snapshot modes](https://github.com/TencentCloud/CubeSandbox/blob/3a1274d68d7778b62dccfc0056b212ee5fb750fa/hypervisor/vm-migration/src/lib.rs#L110)
- [Snapshot/clone/rollback deep dive](https://github.com/TencentCloud/CubeSandbox/blob/3a1274d68d7778b62dccfc0056b212ee5fb750fa/docs/zh/blog/posts/2026-06-25-cubesandbox-snapshot-clone-rollback-deep-dive.md)

### 5.2 Auto-pause 与 Auto-resume

CubeSandbox `v0.5.0` 起提供平台级 Auto-pause/Auto-resume，生命周期参数与 E2B 接近。当前判定链路是：

```text
入站请求完成
  ↓ CubeProxy log_phase.lua
按 sandbox_id 更新 last_active（同一 Sandbox 最多每秒一次）
  ↓ cube-lifecycle-manager 定期拉取并合并
baseline = max(LastActiveMs, CreatedAt)
  ↓ now - baseline >= timeout
auto_pause=true → Pause；否则 Kill
```

Pause 会保存完整 VM/RAM 状态并关闭 MicroVM。若 `auto_resume=true`，之后第一个入站 dataplane 请求被 CubeProxy 的 rewrite gate 截住，内部调用 lifecycle manager → CubeMaster → Cubelet → CubeShim 恢复 Sandbox；成功后才继续转发原请求。并发恢复在单进程内合并，并以 Redis 状态锁协调多个代理副本。也可以通过 `connect()`/显式 Resume 唤醒。

与 E2B 的调研结果一样，这不是 CPU idle detector。**当前唯一持续刷新 idle baseline 的运行期信号，是经过 CubeProxy 并完成的入站请求。**SDK 调用、文件操作和访问 Sandbox 内服务之所以算活跃，是因为它们都走该入站代理链路；VM 内部独立计算和出站网络活动不算。成功 Resume 会主动把内存 registry 的 `LastActiveMs` 刷新到当前时间，避免刚恢复便再次被 sweep。

源码入口：

- [入站请求 activity stamp](https://github.com/TencentCloud/CubeSandbox/blob/3a1274d68d7778b62dccfc0056b212ee5fb750fa/CubeProxy/lua/log_phase.lua)
- [Idle baseline 与 sweeper](https://github.com/TencentCloud/CubeSandbox/blob/3a1274d68d7778b62dccfc0056b212ee5fb750fa/cube-lifecycle-manager/internal/sweeper/sweeper.go#L93)
- [入站请求 Auto-resume gate](https://github.com/TencentCloud/CubeSandbox/blob/3a1274d68d7778b62dccfc0056b212ee5fb750fa/CubeProxy/lua/sandbox_state.lua)
- [Resume 合并与状态刷新](https://github.com/TencentCloud/CubeSandbox/blob/3a1274d68d7778b62dccfc0056b212ee5fb750fa/cube-lifecycle-manager/internal/resumer/resumer.go#L64)

### 5.3 Issue #793：Agent-in-sandbox 的活跃误判

[Issue #793](https://github.com/TencentCloud/CubeSandbox/issues/793) 是一个截至调研日期仍为 **Open** 的 enhancement，不是已经落地的能力。它指出 ingress-only 判定对 Agent-in-sandbox 工作负载存在结构性误判：

- Agent 在 VM 内计算、编译或运行长任务时没有入站请求，会被当成 idle 并在任务中途 Pause；
- Agent 只做出站活动（调用 LLM API、下载资源、push remote）同样不会更新时间戳；
- Pause 会断开对外 TCP 连接，即使 Guest 内 TCP state 被 Snapshot 保存，外部 peer、NAT/conntrack 和超时状态也不会随 VM 一起恢复；
- 当前规避方式只有把 timeout 设得很大、禁用 timeout，或制造入站 heartbeat。

Issue 提议让每个 Sandbox 可配置多种 activity signal，并以所有启用信号的最新时间作为 baseline：

```text
max(
  ingress_last_active,
  in_vm_cpu_or_load,
  egress_last_active,
  agent_heartbeat
)
```

其中 CPU/load、egress 和 Agent heartbeat **均未实现**。维护者在评论中说明，通用触发机制复杂，当前有意简化为 traffic signal，并建议把 Agent 设计为由外部请求驱动的 stateless executor；Issue 作者则指出 Claude Code、OpenClaw、Hermes 等现有 Agent 在进程内持有会话、工具上下文和子进程状态，改成无状态相当于重写。维护者还指出，内置 timer 的 Agent 即使能被流量唤醒，也会遇到暂停期间 timer 语义失效的问题，需要单独设计。

因此 #793 暴露了两个彼此独立的问题：

1. **误暂停（activity detection）**：平台如何知道 VM 内 Agent 仍在工作；
2. **恢复后的外部世界一致性（resume semantics）**：timer、TCP peer、NAT、远端请求和 deadline 如何处理。

增加 CPU/egress/heartbeat 只能缓解第一个问题，不能自动修复第二个问题。对未经改造的长驻 Agent，当前可靠选项仍是 `NEVER_TIMEOUT`/禁用 Auto-pause，或由外部控制器周期性制造入站 heartbeat，并在 Resume 后重建外部连接、重新计算 deadline。

相关资料：

- [Issue #793 正文：问题与多信号提案](https://github.com/TencentCloud/CubeSandbox/issues/793)
- [维护者评论：当前简化为 traffic signal](https://github.com/TencentCloud/CubeSandbox/issues/793#issuecomment-4901778404)
- [维护者评论：timer 与 traffic-driven recovery 边界](https://github.com/TencentCloud/CubeSandbox/issues/793#issuecomment-4902658121)
- [Sandbox lifecycle 文档](https://github.com/TencentCloud/CubeSandbox/blob/3a1274d68d7778b62dccfc0056b212ee5fb750fa/docs/guide/lifecycle.md)

## 6. BoxLite：当前是磁盘 Snapshot

### 6.1 Snapshot 实现

BoxLite 已提供 Snapshot 的 create/list/get/remove/restore，覆盖 Rust、Python、Node.js 和 REST API。它使用 QCOW2 external COW：

```text
Guest FIFREEZE（best effort）
  ↓
SIGSTOP VM shim
  ↓
把当前 writable disk 移为不可变 Snapshot
  ↓
创建以该 Snapshot 为 backing file 的新 COW child
  ↓
SIGCONT VM shim
  ↓
Guest FITHAW
```

运行中可创建 Snapshot；如果 Guest quiesce 失败，则退化为 crash-consistent，不保证数据库等应用已 flush。Restore 要求 Box 停止，并以指定 Snapshot 创建新的 writable child。之后 microVM 和 Guest 冷启动，不能恢复原进程位置。

Snapshot 只覆盖 writable container disk，不包含 external bind mounts、host volumes、远端存储或 RAM。QCOW2 backing chain 有依赖关系，跨主机备份应使用 export/import，而不是单独复制一个 layer。

源码入口：

- [Snapshot API](../../src/boxlite/src/litebox/snapshot.rs)
- [Local QCOW2 Snapshot](../../src/boxlite/src/litebox/local_snapshot.rs)
- [Snapshot manager](../../src/boxlite/src/litebox/snapshot_mgr.rs)
- [Freeze/create/thaw orchestration](../../src/boxlite/src/litebox/box_impl.rs)
- [Issue #205: disk-only Snapshot design](https://github.com/boxlite-ai/boxlite/issues/205)

### 6.2 Pause/Resume 状态

BoxLite 内部已通过 `SIGSTOP`/`SIGCONT` 冻结 VM shim，供 Snapshot、Clone 和 Export 的短临界区使用；`Paused` 也存在于状态机中。其语义类似 freezer：VM process 仍存在、RAM 仍驻留，不能跨 host reboot 或迁移。

用户级 Pause/Resume 正在 [PR #413](https://github.com/boxlite-ai/boxlite/pull/413) 开发，但截至调研日期仍未合并，不应算作主线或 `v0.9.7` 的稳定公开能力。

即使该 PR 合并，也不会自动得到 Memory Snapshot：BoxLite/libkrun 仍缺少完整的 vCPU、VM/device state、RAM layout/content、版本化格式、dirty tracking 和 restore builder 集成。

## 7. 对 BoxLite 的启示

如果目标只是短时零 CPU 挂起，公开现有 freeze/unfreeze 能力即可，但 RAM 无法释放，host 重启后也不能恢复。

如果目标是 E2B 式 Auto-pause/Auto-resume，需要的是完整 checkpoint pipeline：

```text
libkrun/VMM state save/restore
  + Guest RAM Full/Diff Snapshot
  + dirty-page tracking
  + rootfs COW consistency
  + version and CPU compatibility
  + local/remote Snapshot storage
  + lazy page backend and prefetch
  + scheduler and traffic-triggered resume
```

推荐分阶段实施：

1. 合并稳定的同进程 Pause/Resume，明确它只是 freezer 语义；
2. 核实 libkrun 在 Linux KVM 与 macOS HVF 上可持久化的 vCPU/device state 边界；
3. 先实现同主机、同版本、Full RAM 的 cold checkpoint/restore；
4. 再加入 dirty-page 增量和磁盘一致性协议；
5. 最后建设 lazy restore、远端分层存储、调度和 Auto-resume。

不能用 `SIGSTOP`/`SIGCONT` 或单独复制 RAM 文件替代第 2—4 步。

## 8. 参考资料

### Firecracker

- [Snapshot support](https://github.com/firecracker-microvm/firecracker/blob/1faca2f70e7af6672fc7b4daacd9d037c874c335/docs/snapshotting/snapshot-support.md)
- [Snapshot versioning](https://github.com/firecracker-microvm/firecracker/blob/1faca2f70e7af6672fc7b4daacd9d037c874c335/docs/snapshotting/versioning.md)
- [Snapshot create/restore](https://github.com/firecracker-microvm/firecracker/blob/1faca2f70e7af6672fc7b4daacd9d037c874c335/src/vmm/src/persist.rs)
- [Memory dump and mapping](https://github.com/firecracker-microvm/firecracker/blob/1faca2f70e7af6672fc7b4daacd9d037c874c335/src/vmm/src/vstate/memory.rs)

### E2B

- [Architecture: Pause and resume](https://github.com/e2b-dev/infra/blob/5b465bc3e72aaca4a8c9dd80fada0c3854cd4431/docs/ARCHITECTURE.md#pause-and-resume)
- [Pause/Resume orchestration](https://github.com/e2b-dev/infra/blob/5b465bc3e72aaca4a8c9dd80fada0c3854cd4431/packages/orchestrator/pkg/sandbox/sandbox.go)
- [UFFD implementation](https://github.com/e2b-dev/infra/blob/5b465bc3e72aaca4a8c9dd80fada0c3854cd4431/packages/orchestrator/pkg/sandbox/uffd/uffd.go)
- [Traffic Auto-resume](https://github.com/e2b-dev/infra/blob/5b465bc3e72aaca4a8c9dd80fada0c3854cd4431/packages/client-proxy/internal/proxy/proxy.go#L76)

### CubeSandbox

- [Issue #793: activity signals beyond ingress traffic](https://github.com/TencentCloud/CubeSandbox/issues/793)
- [Sandbox lifecycle](https://github.com/TencentCloud/CubeSandbox/blob/3a1274d68d7778b62dccfc0056b212ee5fb750fa/docs/guide/lifecycle.md)
- [Auto-pause sweeper](https://github.com/TencentCloud/CubeSandbox/blob/3a1274d68d7778b62dccfc0056b212ee5fb750fa/cube-lifecycle-manager/internal/sweeper/sweeper.go)
- [Auto-resume gate](https://github.com/TencentCloud/CubeSandbox/blob/3a1274d68d7778b62dccfc0056b212ee5fb750fa/CubeProxy/lua/sandbox_state.lua)
- [CubeShim Pause/Resume](https://github.com/TencentCloud/CubeSandbox/blob/3a1274d68d7778b62dccfc0056b212ee5fb750fa/CubeShim/shim/src/sandbox/sb.rs)
- [Memory Snapshot implementation](https://github.com/TencentCloud/CubeSandbox/blob/3a1274d68d7778b62dccfc0056b212ee5fb750fa/hypervisor/vmm/src/memory_manager.rs)

### BoxLite

- [Issue #205: Box Snapshot API](https://github.com/boxlite-ai/boxlite/issues/205)
- [PR #413: Pause/Resume API](https://github.com/boxlite-ai/boxlite/pull/413)
- [Snapshot implementation](../../src/boxlite/src/litebox/snapshot.rs)
- [Box lifecycle implementation](../../src/boxlite/src/litebox/box_impl.rs)
