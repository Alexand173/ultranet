# The Windows job: what it proves, how long it takes, and how to check it locally

`.github/workflows/windows-tests.yml` has two jobs. `Linux check (all targets)` compiles every target
on a warm cache in **9 m 13 s measured on 2026-11-21** and catches structural drift. `Windows x64 Rust
tests` builds and runs the whole suite for `x86_64-pc-windows-gnu`; it is the only place platform
assumptions are exercised.

Both jobs run on `push` to `main`, on `pull_request` to `main`, and on `workflow_dispatch`. **A push to a
feature branch triggers nothing** — to get a signal before opening a PR, dispatch it explicitly:

```bash
gh workflow run windows-tests.yml --ref <branch>
```

## 1. Baseline durations, measured 2026-11-21

These are the numbers to judge a new run against. A 20% drift is worth reading the log for. Both columns
are a single measured run, not an average, and both are from the API
(`jobs[].steps[].started_at`/`completed_at`), so they exclude queue time.

| Stage | `windows-tests.yml` (serial) | `release.yml` (parallel) | Notes |
|---|---|---|---|
| Setup: checkout, MSYS2, LLVM, Rust, cache, toolchain verify | 286 s | 91 s | MSYS2 install alone is 71-76 s; the cache restore is 1 s warm and 192 s cold |
| Compile, link, package | inside the suite step | 1587 s | `release.yml` builds and packages a binary; `windows-tests.yml` compiles through `cargo test` |
| Full suite | 5597 s | 3300 s | 480+ tests; the two ledger tests still dominate |
| Whole job | 5897 s (98 min) | 3540 s (59 min) | both well inside `timeout-minutes: 180` |

**Where the numbers come from:** the parallel column is `release.yml` run `44884199956` (2026-11-21,
`ci/release-dry-run`, which exercised the whole release path without publishing). The serial column is
`windows-tests.yml` run `44885315754` (2026-11-21, PR #12). An earlier serial run on the pre-hardening
workflow, `44877932589`, took **4730 s (78.8 min)** with a 4429 s suite and a 281 s setup — so the MSYS2
change and the caching moved setup by seconds, not minutes, and the suite duration is what varies.

**The suite alone ranged 4429-5597 s across these two serial runs**, on identical test counts and the same
runner class: 26% spread, which is why a single slow run is not evidence of a regression. Setup, by
contrast, was 281 s and 286 s — the stable part, and the reason a spike there is worth reading.

**What the setup cost actually is.** Of the 286 s, 192 s is the Cargo cache restore and 71 s the MSYS2
install. Caching `target/` buys back far more than it costs, but the restore is the step to watch if setup
grows: it is 67% of setup time.

The parallel release job is 20 minutes shorter than the serial gate and proved the same portability fixes.
The serial run stays the gate anyway: two failures on this runner were concurrency, not logic —
`restart_replay_tests` panicked inside sled with `failed to allocate Arc` while several stores were open at
once (`main` at `451dc86`, run `44762293962`), and the loopback snapshot test lost a start-up race. Two
vCPUs and a thin memory margin mean a flaky check costs more than a slow one; serialising fits inside the
job timeout comfortably.

Locally, on this workstation: the two ledger tests went from **158 s to 94 s** once they stopped copying
a RocksDB directory, and `cargo check --all-targets` takes ~10 s when warm.

## 2. Checking Windows compilation locally, before pushing

A `cargo check` does not link, so it needs no Windows linker — only a C/C++ compiler for the build
scripts. On WSL2 Ubuntu with `gcc-mingw-w64-x86-64` and `g++-mingw-w64-x86-64` installed:

```bash
export CC_x86_64_pc_windows_gnu=x86_64-w64-mingw32-gcc-posix
export CXX_x86_64_pc_windows_gnu=x86_64-w64-mingw32-g++-posix
export CFLAGS_x86_64_pc_windows_gnu="-g0"
export CXXFLAGS_x86_64_pc_windows_gnu="-g0"
export CARGO_PROFILE_DEV_DEBUG=0

cargo check --locked --all-targets -j 1 --target x86_64-pc-windows-gnu
```

Two settings are not optional, and both cost an hour to rediscover:

* **`-posix`, not the default.** The default `x86_64-w64-mingw32-g++` is the win32-threads variant, which
  has no `__gthread_cond_t`; RocksDB's C++ fails immediately with errors in `<bits/std_mutex.h>`.
* **`-g0` / `CARGO_PROFILE_DEV_DEBUG=0`.** With debug info on, `rocksdb/db/db_impl/db_impl.cc` produces an
  object file MinGW refuses to write: `Fatal error: ...db_impl.o: file too big`.

First run compiles RocksDB and takes ~12 minutes at `-j 1`; later runs reuse the cache.

## 3. The rule this job enforces

Anything that exists only on Unix must sit behind a `cfg` gate, and the gate must be visible at the call
site. Before pushing, audit:

```bash
grep -rnE 'unix::|libc::' src/ tests/
```

Every hit must be one of: inside a `#[cfg(unix)]` block, inside a `#[cfg(all(test, unix))]` module, or in
`src/platform.rs` (the one file that is allowed to contain both branches). Four defects in this class have
already reached CI and cost a 15-50 minute cycle each: `Permissions::from_mode` in a test module, the same
call in two binaries' `backup` command, and `tokio::signal::unix` in both service binaries.

`src/platform.rs` holds the three answers so call sites stay clean: `restrict_to_owner` (mode 0600 where
the platform can express it, reporting honestly where it cannot), `describe_owner_only`, and `terminate()`
(SIGTERM on Unix, a future that never resolves elsewhere, with Ctrl-C kept as its own select arm).

## 4. Reading a red run

Do not call it a timeout until the log says so. The job's only deadline is `timeout-minutes: 180`, which
is roughly three times a normal run, so a 50-minute failure is an assertion, not a clock.

1. Read the **step** conclusion, not the run conclusion.
2. Download the artifact `ultranet-windows-test-log-<run_id>` (uploaded only on failure) and grep for
   `^error`, `FAILED`, `panicked at`, `test result:`.
3. `^error` means portability: find the `unix::` call behind it (section 3).
4. `panicked at` means a test assumption: file locking, sockets, permissions, or an absolute path. Ask
   which OS assumption the line makes, not whether the platform is "flaky".
5. `test result:` absent and the last line is a bare test name: only then suspect a hang, and re-run with
   `--test-threads=1 --nocapture`.
6. Compare with `main`'s log at the same commit ancestry before attributing anything to a branch.

Fixtures that assume Unix are the recurring cause: `/opt/...` is not absolute on Windows, `#!/bin/sh`
scripts cannot run there, and a database directory cannot be copied while the store holds it open.
