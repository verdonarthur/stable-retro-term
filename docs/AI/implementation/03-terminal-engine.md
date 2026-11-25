# Terminal engine and PTY sessions

Status: implementation plan ready. Terminal engine code is pending.
Dependency: Task 02 merged into `main` at `0992f7d`.

## Goal and context

Wrap `alacritty_terminal` in a Linux-first, window-independent session API.
Shell output must update terminal state and return protocol replies to the PTY.
Input and resize must reach the PTY. Session close uses Alacritty's worker shutdown
and PTY cleanup.
Use headless tests with fixed expected results and a real PTY.

Read [the architecture](../architecture/retro-terminal.md), [the execution index](../README.md),
and [the development commands](../../development.md). At the implementation base,
`crates/terminal/src/lib.rs` is a stub and `crates/terminal/Cargo.toml` has no dependencies.
`crates/app/src/main.rs` is empty apart from `main`. There are no terminal tests or app callers.
`xtask` already provides `worktree create`, `check`, and `doctor`.

## Settled decisions

- D1. Keep this plan and the implementation handoff in this tracked document.
- D2. Base implementation on merged `main`. The agreed starting commit is
  `0992f7d74e4d898cc799341bf7f5f8fdcaf7c732`, observed at `origin/main` during planning.
- D3. Use shutdown and PTY cleanup from the published `alacritty_terminal` 0.26.0
  crate. This supersedes the 500 ms grace period and forced termination decision.
  The upstream cleanup sends `SIGHUP` and waits for the direct child. A child that
  ignores hangup and keeps running can delay cleanup indefinitely. After natural
  exit, the destructor can send hangup to a reused numeric PID. These upstream
  limitations are accepted for Task 03.
  [PTY cleanup](https://github.com/alacritty/alacritty/blob/v0.17.0/alacritty_terminal/src/tty/unix.rs#L281-L293)
  · [Child reaping](https://github.com/alacritty/alacritty/blob/v0.17.0/alacritty_terminal/src/tty/unix.rs#L350-L370)

Keep the three product crates and the development-only `xtask`. The terminal crate owns
PTY I/O and terminal state. The app will own window events, shortcuts, and OS clipboard
integration in Task 04. Keep Alacritty types private to the terminal crate.
The 1.20× CPU, RAM, and GPU limits against Alacritty still apply independently to all shipped
presets. Static CRT stays the default. Task 05 owns the measurements.

## Start the implementation worktree

From a checkout with the Task 02 command, run:

```sh
rtk cargo xtask worktree create 03-terminal-engine --base 0992f7d74e4d898cc799341bf7f5f8fdcaf7c732
```

Work in the directory printed by the command. If the plan refinement is still uncommitted,
carry this file into that worktree so the fresh session has the settled decisions.
Record the base SHA and worktree path below. Do not commit, push, or open a PR without a request.

## Implementation steps

1. **Select and check dependencies.** Edit `Cargo.toml`, `crates/terminal/Cargo.toml`,
   and `Cargo.lock`. Start with the published `alacritty_terminal` 0.26.0. Its Rust minimum
   is 1.85 and its license is Apache-2.0. Disable its default serialization feature unless
   a use in this task requires it. Resolve the lockfile with Rust 1.94 and run
   `rtk cargo xtask check --step deny` when the pinned tool is available. Check transitives
   against `deny.toml`. Keep the workspace toolchain and lint policy.
   [Published manifest](https://docs.rs/crate/alacritty_terminal/0.26.0/source/Cargo.toml)
2. **Wrap terminal state and expose the visible view.** Add `src/engine.rs` and
   `src/snapshot.rs` under `crates/terminal/`. Keep `Term`, its synchronization,
   and the parser adapter internal. Return an owned snapshot on demand using the visible
   viewport. Include cell positions, colors and attributes, wide-cell markers, combining
   characters, cursor shape and visibility, selection, and display offset. Reuse engine
   selection and scrollback. Hold the terminal lock only while reading or changing its
   state. Expose a coarse change indication that also covers cursor, selection, scroll,
   and resize. Add `tests/terminal.rs` with fixed escape-sequence fixtures and explicit
   cells, cursor positions, and modes. Cover SGR colors, cursor movement, alternate-screen
   restoration, wide and combining characters, and scrollback bounds and selection.
3. **Start a session and signal changes.** Add `src/session.rs` and re-export the public
   surface from `src/lib.rs`. Give each session a stable ID independent of child PID.
   Accept the initial grid and cell dimensions, an optional program with arguments and
   working directory, and a finite scrollback limit. Reuse Alacritty's PTY and one I/O worker.
   Keep one pending wakeup per session and publish state changes such as title, bell, and
   child exit through bounded state. The event consumer must be able to consume and rearm
   a notification during concurrent output. Document that the wake callback schedules work
   and must return promptly without calling back into the session. Start `tests/session.rs`
   with startup failure, separate session IDs, a fixed output marker from a child, and
   notification coalescing with a final update after rearm.
4. **Route input, resize, and replies.** Add `src/input.rs` and extend `src/session.rs`
   and `src/engine.rs`. Accept platform-neutral text, paste, and encoded input bytes.
   Encode paste using the current bracketed-paste mode. Validate dimensions before the
   grid resize and PTY resize. Use the same rows and columns for both. Check the engine's
   size minimums and the pixel products used by the PTY's 16-bit size fields.
   Use the upstream writer for input and replies. Pass through `PtyWrite` and resolve
   text-area and color requests using current size and palette state, including overrides.
   The parser can emit callbacks while it holds the terminal lock. Defer work that needs
   that lock to the session owner's event pump. Keep deferred replies in order in bounded
   storage and surface overload as a session error. Service replies even when the session
   has no visible view. Reuse the upstream write path without adding an unbounded paste
   or output queue. Leave OSC 52 clipboard access disabled for Task 03.
   Test exact bracketed and plain paste bytes, invalid dimensions, an input round trip,
   terminal and PTY size agreement, and exact cursor-position, text-area, and color replies.
   Exercise a query that waits for the reply before the child prints its success marker.
   [Upstream events](https://docs.rs/crate/alacritty_terminal/0.26.0/source/src/event.rs)
5. **Handle natural exit.** In `src/session.rs`, enable the worker's final PTY drain and
   retain the child's exit status. Publish the final snapshot and exit state after the drain.
   The upstream `ChildExit` event arrives before the drain. Use the worker's final exit
   sequence to decide when the result is ready. Join outside terminal and event locks
   and drop the returned loop so it releases the PTY. Surface failures from worker send
   and join operations and document any I/O diagnostics the upstream worker only logs.
   Test a child that writes a final marker and exits with a known nonzero code, plus
   input attempts after exit.
   [Upstream worker and exit flow](https://docs.rs/alacritty_terminal/latest/src/alacritty_terminal/event_loop.rs.html#205)
6. **Implement explicit close and drop.** In `src/session.rs`, send the upstream
   worker its shutdown message, join it, and drop the returned loop to release its PTY.
   Join and release the PTY outside terminal and event locks. Let Alacritty own child
   signaling and reaping. Handle a worker that has already finished and make repeated
   close calls safe. Use the same cleanup path from `Drop` where possible.
   Document that close and drop can block while upstream waits. Test explicit close
   and scope-drop cleanup with a child that responds to hangup. Check worker completion
   and child reaping. Also test repeated close and close after natural exit. Bound the
   test harness waits and keep a cleanup guard for failing tests.
7. **Document the integration contract.** In crate and public API docs, describe session
   ownership, errors, event pumping, snapshot lifetime, input and grid sizing,
   exit and close semantics, and the Linux requirement. Explain which event the app
   should use to redraw and which event means the final output is ready. Test through
   the public API where possible. Use controlled programs and explicit readiness signals
   in PTY tests. Use fixed expected bytes and cell data from the fixture, independent of
   the wrapper's output.

## Acceptance criteria

- PTY output produces the expected terminal state, colors, cursor, and protocol replies.
- Input and resize reach the PTY. Session close stops the worker and reaps a direct
  child that responds to hangup. Natural exit still delivers final output and exit status.
- Terminal tests run without a display or GPU. Fixed expected outputs can be read in
  the fixtures and assertions.

## Verification

Run from the implementation worktree:

```sh
rtk cargo test --locked -p retro-terminal
rtk cargo xtask check --json
```

If a pinned tool is missing, run `rtk cargo xtask doctor`. Report which checks did not
run and obtain full results from CI when a CI run is available. Record the exact commands,
exit codes, and `target/xtask/` artifact paths. Use project tooling only.

## Out of scope

- A new terminal parser, window or GPU code, OS clipboard access, GUI keyboard mapping,
  tabs, splits, and process-tree supervision.
- An Alacritty shutdown patch, a close deadline, and forced termination.
- Performance measurements and claims. Headless checks establish functional behavior.

## Handoff

- The shutdown decision is resolved in D3. The plan now follows upstream behavior.
- Implementation preflight reran `rtk cargo xtask check --json` and
  `rtk cargo xtask doctor` in the planning checkout. Both exited 3. The check passed
  format, compile, Clippy, and baseline tests. Deny, actionlint, and zizmor did
  not run because their tools were missing. The focused Task 03 tests are pending.
- Planning checkout: `t3/plan-terminal-engine` at `19094cf`. Task 02 merge confirmed
  on `origin/main` at `0992f7d`. Create the implementation worktree from that merge.
- Planning change: this document. The implementation worktree and terminal code are pending.
- Planning baseline: `rtk cargo xtask check` passed format, compile, Clippy, and tests.
  It exited 3 because `cargo-deny`, `actionlint`, and `zizmor` were missing.
  `rtk cargo xtask doctor` confirmed these missing tools and exited 3.
- Planning artifacts: `target/xtask/check.json`, `target/xtask/doctor.json`,
  and `target/xtask/logs/` in the planning checkout.
- Pending: dependency audit, Task 03 tests, CI, and performance checks.
- At implementation handoff, record the base SHA, worktree, changed files, decisions
  and deviations, exact check results, unrun checks, artifacts, and blockers.
  Update this plan and the execution index based on that evidence.
