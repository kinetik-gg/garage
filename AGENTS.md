# Garage house rules

This file belongs at the repository root. It governs repository-wide process, while
`docs/*.md` is reserved for product documentation whose citations are checked against the
implementation. [convention]

## 0. Scope and precedence

This is the process rulebook for humans and agents working in Garage. For why the code is shaped
as it is, the authority order is rustdoc, then `docs/ARCHITECTURE.md`, then this file; this file
owns process even when the architecture guide does not repeat it. [convention]

## 1. Read before changing anything

Read `docs/ARCHITECTURE.md` §1 for the three layers and one-writer rule, §4 for the render/apply
split and the lock/restart deadlock, and §8 for the do-not-touch table before editing related code.
Follow the cited rustdoc from those sections before changing a load-bearing shape. [convention]

## 2. Validation litany

Run these from `backend/` unless the rule names another location:

- `cargo fmt --all --check`. [enforced: `.github/workflows/ci.yml` / `Rust` / `Check formatting`]
- `cargo clippy --workspace --all-targets -- -D warnings`. [enforced: `.github/workflows/ci.yml` / `Rust` / `Lint`]
- `cargo test --workspace --all-targets`. [enforced: `.github/workflows/ci.yml` / `Rust` / `Test`]
- Run `bash -n` on every shell file touched. CI repeats this for `bootstrap.sh`, `install.sh`,
  `install/lib.sh`, and every numbered install stage; shell elsewhere remains the author's
  responsibility. [enforced: `.github/workflows/ci.yml` / `Shell` / `Check shell syntax`] [convention]
- Smoke the changed behavior through its real entry point and report exactly what ran. A bootstrap
  change also has the non-interactive fresh-Arch path, but that does not replace a focused live
  smoke. [enforced: `.github/workflows/rehearsal.yml` / `Fresh Arch bootstrap` / `Run the bootstrap non-interactively`, for bootstrap] [convention]

## 3. The lint wall

`unsafe_code` is forbidden. Clippy denies `all`, `unwrap_used`, `expect_used`, `panic`, `todo`,
`unimplemented`, `dbg_macro`, `indexing_slicing`, `string_slice`, `unwrap_in_result`, `exit`,
`too_many_lines`, `too_many_arguments`, and `wildcard_enum_match_arm`. [enforced: `backend/Cargo.toml` / `[workspace.lints]`; `.github/workflows/ci.yml` / `Rust` / `Lint`]

CI's `-D warnings` also makes the configured warning set fatal: `unreachable_pub`,
`missing_debug_implementations`, `rust_2018_idioms`, `pedantic`, `cognitive_complexity`, and
`excessive_nesting`. Every workspace member inherits the wall with `[lints] workspace = true`;
the lint-group entries have priority `-1`, so the named lint levels win during Cargo's lint
resolution. [enforced: every `backend/crates/*/Cargo.toml` / `[lints]`; `backend/Cargo.toml` / lint-group `priority = -1`; `.github/workflows/ci.yml` / `Rust` / `Lint`]

## 4. File shape

Keep every `backend/crates/**/src/**/*.rs` file at or below 500 lines. The exception table now lives
in `backend/crates/garage-core/tests/workspace_shape.rs` as `FILE_SIZE_EXCEPTIONS`; its sole current
entry is `backend/crates/garage-core/src/schema/prefs.rs`, because splitting the schema table would
hide drift. Add an exception there only with its reason. [enforced: `.github/workflows/ci.yml` / `Rust` / `Test`; `backend/crates/garage-core/tests/workspace_shape.rs` / `crate_source_files_stay_within_the_line_cap`]

Shell, Lua, and QML are not line-counted. Keep numbered `install/` stages small and single-purpose;
the stage inventory check enforces numbering and bootstrap inclusion, not good decomposition.
[enforced: `.github/workflows/ci.yml` / `Shell` / `Check the numbered install stages`, for inventory]
[convention]

## 5. Tests

`cargo test --workspace --all-targets` is the repository test suite. Put behavioral contracts in
crate tests so the normal suite and CI run them. [enforced: `.github/workflows/ci.yml` / `Rust` / `Test`]

Two repository guards are Cargo integration tests in
`backend/crates/garage-core/tests/workspace_shape.rs`: `render_crate_cannot_reach_preferences_lock_or_process_execution`
rejects direct or transitive `garage-render` dependencies on `garage-prefs` or `garage-proc`, and
`crate_source_files_stay_within_the_line_cap` enforces the Rust file cap and exception table.
[enforced: `.github/workflows/ci.yml` / `Rust` / `Test`; `backend/crates/garage-core/tests/workspace_shape.rs`]

The retired Python differential suite is history: there is no golden oracle now; the behavioral
contract rests on Cargo tests, the three CI jobs, and the fresh-Arch rehearsal workflow.
[enforced: `.github/workflows/ci.yml` / jobs `Rust`, `Shell`, and `Repository sync`; `.github/workflows/rehearsal.yml` / `Fresh Arch bootstrap`]

## 6. Documentation and install inventory

Do not cite Rust by line number; cite the stable symbol and explain the invariant. In `docs/*.md`,
every backticked `name()` citation must resolve to a Rust function or one of the workflow's explicit
Lua allowlist entries, and raw `garage:<line>` citations are rejected. [enforced: `.github/workflows/ci.yml` / `Repository sync` / `Check documentation citations`]

Keep the “What bootstrap does” table in `docs/INSTALL.md` exactly synchronized with the numbered
`install/[0-9][0-9]-*.sh` files, including its `bootstrap.sh` summary row. [enforced: `.github/workflows/ci.yml` / `Repository sync` / `Check the install table against the stage files`]

## 7. One writer

`save_preferences()` is the only settings-path writer of `preferences.toml`, and it writes only
departures from shipped defaults. The three deliberate off-path writers are
`compact_preferences_file()` for the v5 rewrite, `repair_reset()` for `garage repair --reset`, and
bootstrap's first-boot GPU glass gate, which writes only when the file is absent; each emits only a
departures-only or stamp-only document. Review any proposed fourth writer against the rustdoc and
`docs/ARCHITECTURE.md` §1. [convention]

`save_workspace_blocks()` and `keybind_action()` are the single writers of
`workspace-blocks.toml` and `keybindings.toml`. `displays.toml` deliberately has two serialized
writers: `display_finish()` and the non-overwriting first-apply seed
`initialize_display_config()`, both under `DisplayLock` and the same normalization/serialization.
[convention]

Renderers read layers 1 and 2 and write layer 3. The documented exception is the workspace-block
allocator: render and snapshot may persist a newly seen connector through the one
`save_workspace_blocks()` path because the allocation must survive the render that created it.
[convention]

## 8. Shell or port

Before replacing or expanding a shell script, classify it as `pure exec wrapper (STAYS SHELL)`,
`already a shim`, or `real logic (port candidate)`. Port reusable state/decision logic; keep launch,
TTY, signal, and process choreography in shell when that choreography is the job. Record every
script considered in the commit body, including explicit stays and no-ops, with this verdict table:
[convention]

| Script | Classification | Action | Why |
| --- | --- | --- | --- |
| `path-or-command` | `classification` | `action` | `reason the boundary belongs here` |

## 9. Migrations or convergence

One-shot transformations of machine state that a checkout cannot derive belong in
`garage migrate`; state derivable from the checkout belongs in idempotent bootstrap/reconcile
convergence. Every registered migration must be idempotent, safe to retry before its stamp lands,
and a no-op on a fresh install. [enforced: `.github/workflows/ci.yml` / `Rust` / `Test`; `backend/crates/garage-apply/src/migrations/mod.rs` / `every_registered_migration_is_a_no_op_on_a_fresh_install`]

## 10. Dry runs

Route shell mutations through `run()`; use `write_file()` only for heredoc/redirection bodies that
`run()` cannot wrap. A Rust `--dry-run` may inspect and describe but writes nothing: no transcript,
stamp, backup, ledger, or target mutation. Add the byte-identical-tree test before trusting a new
write path. [convention]

## 11. Git

Use an imperative Conventional Commit subject and a body that says why. Stage an explicit file list;
never use `git add -A` or `git add .`. Never run `git stash`: isolate concurrent work with
`git worktree`. Preserve unrelated worktree changes, add the co-author trailer for the agent that
did the work, and push only through the repository's CI path. [convention]

## 12. Agent operations

- Do not send subagents into the user's memory vault; pass them the bounded repository context they
  need. [convention]
- Killing a wrapper does not prove its child died. Check the child PID/process group before
  re-dispatching the work. [convention]
- Never use `pkill -f` with a pattern that appears in the command line doing the killing; resolve
  the intended PID or process group first. [convention]
