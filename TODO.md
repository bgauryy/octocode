# TODO: cleanups for tools, skills and tests

Open cleanup work, each with its owner area and the check that closes it. Remove an item when its check passes.

## Rule: temp data lives in `.octocode/tmp/<task>/`

Every scratch file, test or probe home, log of one run, frozen binary and harness output goes under
`<workspace>/.octocode/tmp/<task>/` (gitignored) and is removed when the run ends
([AGENTS.md](AGENTS.md#best-practices)). Deliverables a user reopens (reports, RFCs, saved runs) stay in
`<workspace>/.octocode/<skill>/`. Exceptions are listed below with the reason; nothing else writes to `/tmp`,
`$TMPDIR` or an agent scratchpad.

Found on 2026-11-24: about 32 GB of leftovers in the user temp dir and agent scratchpads. A search found 272
Rust test files, about 300 JS/TS/Python files and 5 docs writing temp data outside `.octocode/tmp`; 6 sites never
cleaned up.

### In progress (this round)

| Area | What | Done when |
|---|---|---|

Landed this round, checks passed: core `mcp-host` removes its runtime root on close and exit (573 tests; 0 new
`octocode-mcp-host-*`); the brainstorming, messages-proxy, pr-review and gates leaks; skill scratch now
`.octocode/tmp/<skill>/` in 31 skill docs, `skill-review.mjs` (`scratch-outside-skill-folder`) and the shared
report checks; the graph test root, chat test cleanup and frozen-image pruning (24 h retention, never an image a
live process uses); benchmark docs name `.octocode/tmp/<task>/` (`skills:check`, `docs:verify`, 366 graph tests).

Also landed: JS/TS test scripts of config, mcp, mcp-cli, octocode, vscode, both plugins, chrome-devtools and
native's `test:scripts`/`test:node` run through `scripts/test-tmp.mjs` (`.octocode/tmp/t/<package>-<pid>/`,
removed on pass, fail and SIGINT/SIGTERM; 11 tests); `octocode-mcp-cli` `connectStdio` passes the parent's temp
location to the server it starts, so Chrome in chrome-devtools tests no longer writes to the user temp dir.

- [ ] **`packages/octocode-agents-communication` is not wired to the wrapper.** `src/runtime/workspace.ts:32`
  `coordinationScope()` walks up to the nearest `.git`, so a test workspace inside the repo writes into the
  monorepo's `.git/octocode-communication/` (`documents.ts:51`); no env var stops it. Fix the scope to honor a
  boundary (`GIT_CEILING_DIRECTORIES` or an explicit root), then wire `test` to the wrapper. **A wrapped run already
  left 44 files in `.git/octocode-communication/`** (born 2026-11-24 19:51:32–19:52:37; the two Oct 23 documents
  there are real): remove only those.
- [ ] **Checkout path limit.** The wrapper root fits unix sockets for checkout paths up to 31 bytes (this repo: 27);
  deeper worktrees are refused. Dropping the package name from the root buys 9 bytes.
- [ ] **A SIGKILL of the wrapper leaves its root**; the next run could sweep roots whose pid is gone.
- [ ] **`packages/octocode-native/scripts/test-tmpdir.cjs`** duplicates the wrapper's stop-and-remove logic: move it
  onto the wrapper once the Rust root can live in the repo.
- [ ] **Graph tests freeze fake CLIs into the real user cache** (`packages/octocode-graph`, `octocode-graph-cli-<uid>`):
  contained once the test-runner wrapper sets `TMPDIR`, and pruned after a day. Done when a graph test run adds
  nothing to the user temp dir.
- [ ] **`skills/octocode-pr-review/scripts/test-at-base.mjs:129`** keeps the base worktree in the OS temp dir on purpose
  (removed on success, failure and interrupt): inside the checkout the base run would resolve head's
  `node_modules` from parent folders.
- [ ] **Leftovers from before the fixes**, still in the user temp dir (deleting them was blocked by the permission
  check): 246 `octocode-mcp-host-*`, 741 `search-home-*` and `search-workspace-*`, 47 `octocode-gates-*`,
  `test-at-base-logs-*`, `octocode-proxy-*.log`, `gv-chat-*`, `octocode-symlink-*`, `communication-path-link-*`.

### Needs a design decision

- [ ] **Rust test temp root.** `packages/octocode-native/scripts/test-tmpdir.cjs:27` gives every `test:rust` and
  `test:dev` run a private `/tmp/octocode-rust-test-*` root and removes it at the end. It cannot move into the repo
  yet: the serve tests put a socket about 52 bytes below `TMPDIR`, macOS allows 104 bytes, and
  `<repo>/.octocode/tmp/` is already 42 bytes (worktrees are longer). Options: a socket-dir setting for
  `octocode serve` so tests can keep sockets short and data in the repo, or a short per-run root. Bare `cargo test`
  bypasses the runner and leaks (`octocode-engine-test-*`, `octocode-tool-test-*`); route it through the runner or
  give `test_support` the same root. Done when a full `test:rust` adds nothing to the user temp dir.
- [ ] **`packages/octocode-agents-communication/src/runtime/proxy.ts:64`.** The vendor worker's private cwd
  `octocode-worker-*` is removed in `finally` but survives killed runs (11 left). An installed runtime has no
  repo: candidates are `OCTOCODE_HOME/tmp` or the database directory, swept at startup.
- [ ] **Shipped skill self-tests** (`octocode-skills/scripts/skill-review.mjs:544`, `skill-sync.mjs:241`,
  `scripts/lib/report/self-test.mjs:239` and its copies) run on installed users' machines with no repo. Decide
  whether they use `<workspace>/.octocode/tmp/` or keep the OS temp dir with cleanup.
- [ ] **`packages/octocode-native/crates/engine/benches/search.rs:33`** keeps a 3000×20 corpus in `$TMPDIR` between
  runs on purpose. Move it to `target/` or `.octocode/tmp/bench/`.

### Kept outside `.octocode/tmp`, on purpose

These run for installed users, who have no repository, or are bound by the unix-socket length limit. Each must
stay private per user and clean up after itself.

| Where | What | Why it stays |
|---|---|---|
| `crates/cli/src/cli/serve/unix/sockets.rs:16` | `octocode serve` sockets under `<temp>/octocode-<uid>/` | socket path limit; clients find the server through `TMPDIR`; stale sockets are swept |
| `src/runtime/owner-lock.ts:35` (agents-communication) | owner-lock socket under `/tmp` | socket path limit |
| `scripts/build-slot.cjs:48` (native) | `octocode-build-slots` | shared on purpose by every checkout and worktree on the machine |
| `crates/cli/src/cli/config_view/browser.rs:161` | kiosk browser profile | installed CLI; removed on drop |
| `src/runtime/lib/pin-cli.ts:53` (octocode-graph) | frozen executable images | installed package, shared across repositories; pruned (see above) |
| `OCTOCODE_HOME/tmp/*` (clone, materialize, rate-limit, locks) | the user's Octocode home, not a repo | swept by `crates/runtime/src/runtime/maintenance.rs:26` |
| atomic writes (`crates/io/src/atomic.rs:94`, launchers, vscode) | sibling `*.tmp` beside the target | must be on the same filesystem as the target |

### Outside this repository

Prefixes seen in the user temp dir that come from sibling repos; tell their owners:
`~/code/octocodecode` (`testkit-*`, `tk-mem`, `fake-tmux`, `octocode-context`, `octocode-real-process`,
`octocode-native-reliability`, `octocode-browser-smoke`, `octocode-publish`) and `~/code/octocode-sandbox-runtime`
(`srt-ca`, `srt-vmon`, `srt-git-ssh`).

## Other tool cleanups

- [ ] **Test failures that predate this round** (seen with and without the wrapper): both plugin `package.test.ts`
  (`skills/octocode-agentic/references/interview.md` links `scripts/tests/interview-grade.test.mjs`, which the pack
  excludes); native `tests/build-slot.test.cjs:173` fails whenever `OCTOCODE_LANE` is set; `@octocodeai/config`
  `test:coverage` misses its floors (functions 97.91% of 100%, statements 97.19% of 98%).

- [ ] **Engine `rank` and `is_declaration()`** feed only tests since localSearch reads declarations from the outline
  (`crates/engine/src/search/declaration.rs:242-246`), but `rank` is written into the stored scan format
  (`crates/tool-local/src/local_search/packed/write.rs:136`). Removing it changes that format: decide, then remove
  with the format version bump.
- [ ] **Large benchmark clones were deleted** (`local/repos/huge-*`). `questions/local-core` Q17, Q24 and Q25 need
  them: re-clone per `packages/octocode-benchmark/local/README.md` before a local-core round.
- [ ] **A CLI call once failed `ghCloneRepo` with "git is not installed or not on PATH"**; the next identical call
  cloned. Not reproduced. Suspected, unverified: the CLI reached a warm `octocode serve` another session started
  with a different environment.
- [ ] **Instruction room for a `limit` clause** ([RFC P7](docs/rfcs/context-efficiency.md)): the instructions are at
  4,078 of 4,096 characters with every tool on.
