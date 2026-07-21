# AutoPause / AutoResume / AutoDelete Design

## Scope

The first phase reuses the existing `Stop` / `Start` paths. It does not create memory snapshots, does not add a `Paused` state, and does not promise retention of memory, processes, terminal sessions, or network connections. Persistent disks and volumes continue to be managed by existing storage lifecycle policies.

The feature is implemented by the cloud control plane. The REST backend forwards policies to the runner; the embedded local backend does not run a sweeper and returns `BoxliteError::Unsupported` for explicit non-default lifecycle configurations, to avoid silently ignoring them.

## Public Contract

The public wire fields are unified as:

```text
auto_pause:  integer seconds, 0 disables
auto_delete: integer seconds, 0 disables
```

The default `auto_pause` is `900` seconds and the default `auto_delete` is `0`. Create, read, the Rust runtime, and the Python, Node.js, C, and Go SDKs all use the same second-level semantics.

The internal database columns are `autoPause`, `autoDelete`, `autoResume`, and `lastActivityAt`. Public names remain stable.

## State Machine

```text
STARTED -- idle deadline --> STOPPING --> STOPPED
   ^                                    |
   |------ user operation / Start ------|

STOPPED -- delete deadline --> DESTROYING --> DESTROYED
```

AutoPause only selects boxes that satisfy all of the following:

- `state = STARTED`
- `desiredState = STARTED`
- `pending = false`
- `autoPause > 0`
- The last activity time is older than the configured interval in seconds

AutoDelete only selects boxes that satisfy all of the following:

- `state = STOPPED`
- `desiredState = STOPPED`
- `pending = false`
- `autoDelete > 0`
- `lastActivityAt` is older than the configured interval in seconds

`lastActivityAt` is the shared timing source for both AutoPause and AutoDelete. It is updated on box creation, state transitions, and organization changes; therefore AutoDelete starts counting from when the box actually entered `STOPPED` (or from the last event considered activity after stopping), not from when the Stop request was issued.

`auto_delete = 0` means disabled; the legacy `-1` disable semantics and the "delete immediately on stop" semantics are no longer supported.

## Activity Policy

The HTTP runner proxy uses an explicit policy instead of an implicit default at a shared entry point:

| Path | activity | autoResume |
|---|---:|---:|
| Exec and execution controls | true | true |
| Files | true | true |
| Metrics | false | false |

WebSocket attach can trigger AutoResume, but the upgrade itself does not refresh activity. Activity is written only after the proxy is established and a non-empty client data frame is received. Activity writes are throttled by the Redis lock TTL and later flushed to the database by a periodic task.

Standalone port proxies do not write activity and do not trigger AutoResume, so metrics scrapes, health checks, and external port traffic cannot keep a box running indefinitely.

## Strict AutoResume Gate

HTTP Exec/Files and WebSocket attach share `BoxAutoResumeService`:

1. Resolve the canonical box ID and organization.
2. Activity-bearing operations first write the time to the Redis buffer.
3. Acquire the same per-box state lock used by the lifecycle sweeper.
4. `STARTED` passes through directly; `STOPPED` submits a Start intent via a conditional update; in-flight Start requests join the wait; in-flight Stop requests wait for `STOPPED` first, then submit Start.
5. Release the short critical-section lock and do not hold it during cold start.
6. Wait for the actual `STARTED` state via Redis state events, up to 30 seconds.
7. Forward to the runner only after the box has successfully reached `STARTED`.

The waiter allows multiple subscribers for the same box and re-reads state after subscribing, avoiding lost events between the first read and event subscription. Timeouts return an error, not the last observed non-target state.

The distributed lock carries a random owner token and is released through a Redis Lua compare-and-delete; an expired lock from an old worker cannot accidentally delete a lock acquired by a new owner.

## Sweeper and Concurrency Safety

AutoPause and AutoDelete run every 10 seconds and each uses a global worker lock. Candidate boxes must also acquire a per-box state lock.

Activity is written to Redis first and flushed to the database in batches, so relying only on SQL candidates creates a stale window of at most one flush cycle. AutoPause re-reads the Redis-preferred latest time via `BoxActivityService.getLastActivityAt` after acquiring the per-box lock; if there has been recent activity, it skips the box.

State writes use conditional updates:

- AutoPause compares `pending`, `state`, `desiredState`, and the `autoPause` value at selection time.
- AutoDelete compares `pending`, `state`, `desiredState`, and `autoDelete`.

Therefore, if a user changes the policy, manually starts or stops the box, or another worker commits a state change after the candidate query, the stale candidate cannot overwrite the new state.

## Removed Fields and Endpoints

- Legacy minute fields `autoStopInterval` and `autoDelete` have been removed.
- Legacy second field `autoDeleteIntervalSeconds` has been renamed to `auto_delete`.
- `POST /box/{boxIdOrName}/autostop/{interval}` and `POST /box/{boxIdOrName}/autodelete/{interval}` endpoints have been removed.

## Backend and SDK Boundary

`BoxLifecyclePolicy` is validated in the Rust core layer for sentinel and cross-field ordering. The REST runtime:

- `create` puts explicit options in the REST body;
- `Box` responses are mapped to `BoxInfo`.

The local runtime:

- Preserves existing behavior when no lifecycle policy is explicitly configured;
- `create` / `get_or_create` returns `Unsupported` when either lifecycle option is supplied.

The C ABI uses `uint32_t` for `auto_pause` and `uint32_t` for `auto_delete`, with `0` meaning disabled in both cases. The Go bridge, Python, and Node bindings use corresponding non-negative integer types.

## Observability and Failure Semantics

- AutoResume database, lock, and state failures are propagated to the caller; best-effort forwarding is not allowed.
- Suspended organizations keep the same 403 boundary used by explicit Start.
- WebSocket upgrade failures close the socket; HTTP errors preserve the control plane status code.
- Metrics and port access do not produce spurious activity.
- The runner remains the source of truth for actual box state; the control plane only writes desired state, and state synchronization updates the actual state and `lastActivityAt`.
