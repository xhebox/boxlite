# CLI Development Guide

This guide covers building, testing, and contributing to the BoxLite CLI (`boxlite`). The CLI is implemented in the `boxlite-cli` crate and follows the same [Rust Style Guide](./rust-style.md) as the rest of the project.

## Building the CLI

From the repository root, use the canonical full-runtime entrypoints:

```bash
# Matching debug runtime and CLI
make cli

# Matching release runtime and CLI
make cli:release
```

Each target builds the matching runtime profile before compiling the CLI. The binaries are produced at `./target/debug/boxlite` and `./target/release/boxlite`, respectively.

Run the debug build with `./target/debug/boxlite --help`. Use direct Cargo commands when working on the CLI crate without rebuilding the full runtime:

```bash
cargo build -p boxlite-cli
cargo build -p boxlite-cli --release
```

## Testing

### `make test` vs `make test:integration:cli`

- **`make test`** runs the strict full matrix (unit + integration) across core and SDK suites.
- **`make test:integration:cli`** runs only the CLI integration tests. It depends on `runtime:debug` and then:

  ```bash
  cargo test -p boxlite-cli --tests --no-fail-fast -- --test-threads=1
  ```

When working on the CLI, run both as needed:

```bash
make test
make test:integration:cli
```

CLI tests are integration tests: they pull images, create boxes, and run real commands. They require a working VM environment (KVM on Linux or Hypervisor.framework on macOS). Tests use a shared test home (`/tmp/bl`), a global lock to avoid concurrent use, and pre-pulled images (`alpine:latest`, `python:alpine`) to reduce rate limits.

### CLI test layout

- **Entry points:** `boxlite-cli/tests/*.rs` — one file per command (e.g. `run.rs`, `create.rs`, `exec.rs`, `list.rs`).
- **Shared setup:** `boxlite-cli/tests/common/mod.rs` provides `boxlite()` returning a `TestContext` that:
  - Uses `CARGO_BIN_EXE_boxlite` and `--home` pointing at a shared directory (e.g. `/tmp/bl`).
  - Uses a global lock so tests don’t run concurrently against the same home.
  - Pre-pulls images, sets a timeout (e.g. 60s), and exposes `cleanup_box` / `cleanup_boxes`.

Tests use `assert_cmd::Command` and `predicates` to assert exit codes and stdout/stderr. New tests should use `common::boxlite()` and clean up any boxes they create (e.g. with `--rm` or `ctx.cleanup_box(...)`).

Example pattern:

```rust
#[test]
fn test_run_exit_code_success() {
    let mut ctx = common::boxlite();
    ctx.cmd
        .args(["run", "--rm", "alpine:latest", "sh", "-c", "exit 0"]);
    ctx.cmd.assert().success();
}
```

## Code structure

- **Entry:** `boxlite-cli/src/main.rs` — parses the CLI and dispatches to `commands::*`.
- **Subcommands and flags:** `src/cli.rs` — clap definitions: `Cli`, `Commands`, `GlobalFlags`, `ProcessFlags`, `ResourceFlags`, `ManagementFlags`.
- **Command implementations:** `src/commands/*.rs` — each command has an `execute(args, global)`; they share `global.create_runtime()` and similar helpers.

### Adding a new subcommand

1. Add a new variant to `Commands` in `src/cli.rs` and the corresponding `Args` type (or reuse existing flags).
2. In `src/commands/mod.rs`, add the new module and export the `execute` function.
3. In `src/main.rs`, add a branch in `run_cli` that calls the new command’s `execute`.
4. Add tests in `boxlite-cli/tests/<command>.rs` and run `make test:integration:cli`.

## Command reference

| Command        | Description |
|----------------|-------------|
| `make cli`     | Build the matching debug runtime and CLI. |
| `make cli:release` | Build the matching release runtime and CLI. |
| `make test:integration:cli` | Run CLI integration tests (single-threaded). |
| `make test`    | Run strict full matrix (unit + integration across core + SDK). |
| `make test:integration:core` | Run core integration suites (Rust + CLI). |
| `make fmt`     | Format all supported language surfaces. |
| `make lint:rust` | Run Rust lint checks (clippy). |

## See also

- [Rust Style Guide](./rust-style.md) — coding standards for BoxLite.
- [CONTRIBUTING.md](../../CONTRIBUTING.md) — general contribution workflow.
- [boxlite-cli/README.md](../../boxlite-cli/README.md) — user-facing CLI documentation.
