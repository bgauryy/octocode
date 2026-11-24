# Testing environment

How to set up the agent, its tools and its machine before a check or a benchmark round. A result is only as
good as the isolation behind it: an agent that sees your project instructions, memory or a previous answer is
measuring those, not the tool. Method and judging are in [METHOD.md](METHOD.md); this page owns the setup.

## One rule

**Give the agent exactly the tools of its arm, a fresh context, a pinned build and the question. Nothing else.**
Every difference between two arms must be the thing you are comparing.

## Choose the agent by the job

| Job | Agent | Fresh per | Tools | Sees |
|---|---|---|---|---|
| Research answer (one arm) | runner | question **and** arm | the arm's tools only | [runner brief](briefs/runner.md), one [arm brief](briefs/arms/), the question |
| Blind judge | judge | question (swap-confirm with another fresh judge) | read-only shell and the source at the cited commit | [judge brief](briefs/judge.md), the question, answers X and Y, the rubric if the set has one |
| Capability or tool check | checker | tool group | the tool under test, plus the expert tool it is compared with | the tool's [questions](questions/tools/README.md), [check-local](briefs/check-local.md) or [check-github](briefs/check-github.md) |
| Independent rating | rater | lane | its lane's tools, plus read-only ground-truth tools | [independent-raters brief](briefs/independent-raters.md) |
| Skill check | tester | request | the host's real skill loading and the tools the skill names | [SKILLS.md](SKILLS.md) |
| Coordinator | you or one long-lived agent | round | everything | nothing of a runner's context: it merges, never answers |

- **Never reuse a context.** A second question in the same session inherits the first one's findings, cache hits
  and phrasing. One agent, one question.
- **The coordinator is not a runner.** It pins the build, hands out briefs, collects transcripts, and keeps the
  mapping of X/Y to arms private until grading is sealed.
- **Judge from another model family than the runners** where possible, and record both models.
- **Same model, same budget, same frozen refs for every arm in a comparison.** Different days or models are
  labelled as such, never pooled.

## Tools per arm

Give the agent a tool allowlist, not instructions to avoid tools. A rule the agent can break is not isolation.

| Arm | Allowed | Denied |
|---|---|---|
| Octocode (MCP) | the Octocode MCP tools the round names (state whether `clasify`, `graphBuild`, `graphQuery` are in scope) | built-in file read, grep, glob, web, any other MCP server, shell |
| Octocode (CLI) | one shell tool restricted to `octocode …` | everything else |
| gh / gh-lean | shell restricted to read-only `gh` (GET), `jq` and the lean projection tools | `gh` writes, other search tools |
| gh + RTK / Headroom | the same `gh`, behind the compressor | direct unfiltered path |
| Local shell | read-only `rg`, `grep`, `find`, `ls`, `cat`, `sed -n`, `head`, `tail`, `wc`, `sort`, `uniq`, `cut`, `git` read commands, `ast-grep` | writes, network, Octocode |
| Judge | read-only shell, `gh api` GET | writes, the runners' tools |

With Claude Code as the host, `--tools`, `--allowedTools`/`--disallowedTools`, `--mcp-config` with
`--strict-mcp-config`, `--setting-sources`, `--disable-slash-commands`, `--bare` and `--no-session-persistence`
express most of this; other hosts have their own equivalents. **Probe the limit with the agent's real tool
identity** before the first question: ask it to read a path it must not read (a rubric, a sibling question, another
arm's output) and confirm a denial. A directory name alone is not isolation. If a limit cannot be enforced, write
it into the round's record instead of claiming a clean run.

## What a fresh agent must not inherit

- Project instructions (`AGENTS.md`, `CLAUDE.md`), user memory, skills, slash commands, hooks, previous
  transcripts, scratch files from earlier rounds.
- Octocode state from another session: a running `octocode serve`, caches, saved plans, graph homes.
- Rubrics, answer keys, other arms' answers, the question set's README, results of earlier rounds.
- The overfit trail: the question's distinctive terms in the served tool instructions. Search `octocode schema`, the
  MCP instructions and the briefs for each question's repository names, paths and identifiers before a round.

Skill and plugin checks are the exception: there the skill is the thing under test, so load exactly that skill
through the host's real mechanism and nothing else.

## The build

1. **Pin one binary.** Record `octocode --version`, the native binary's SHA-256, the commit and whether the tree is
   dirty. A rebuild during a round invalidates it. Use the host target of `build:dev` for capability checks; time
   **release** builds only.
2. **Call that binary directly.** `NODE_ENV=development OCTOCODE_NATIVE_BIN=<abs path> node packages/octocode/out/octocode.js <tool> …`.
   Without `NODE_ENV=development` the launchers ignore the override and run the installed binary (stderr says so).
   An MCP server already running in your own session serves the build it started with; start a new one from the
   pinned binary for the run (`octocode mcp`) and restart it after every rebuild.
3. **Preflight every arm.** `--version` of each tool, `gh auth status`, and one real call per arm that returns an
   answer. A failed preflight invalidates the round.
4. **Save the served instructions.** Write the `instructions` from the MCP initialize result to a file. That file
   plus each tool's `--help` and `schema` is all the primer an Octocode runner gets.

## Octocode home, config and credentials

| Setting | Do | Why |
|---|---|---|
| `OCTOCODE_HOME` | a new empty directory per round (per agent when agents run in parallel) under `<repo>/.octocode/tmp/<round>/` (gitignored), such as `.octocode/tmp/<round>/home-<agent>/` | no caches, saved plans or config from earlier runs; first-call and repeat-call costs stay separable |
| Config and `.octocoderc` | none beyond what the round states; record any env var you set (`OCTOCODE_ENABLE_LOCAL`, `OCTOCODE_ENABLE_CLONE`, `OCTOCODE_SECURITY_CHECKS`, …) | an arm's tools must match what a user would have |
| GitHub access | one token for the round, via `GH_TOKEN` or `octocode auth login`; a fresh `OCTOCODE_HOME` has no stored login | stored credentials live in `<OCTOCODE_HOME>/credentials.json`: do not copy it into a report or share it between arms |
| Workspace | run from a neutral directory, or the clone itself, with no `.octocode/` of your own | workspace config is read from `<cwd>/.octocode/` |
| Cache state | label every timing **cold** (nothing running, empty home) or **warm** (session server running); never mix | a repeat call answers from cache |

## Repositories and refs

- **Local:** pinned clones under `local/repos/`, set up per [local/README.md](local/README.md). Check
  `git -C <dir> rev-parse HEAD` and `git -C <dir> status --short` before the run. A different commit or a dirty
  tracked tree makes answers not comparable with stored ones. Research arms never write to a clone.
- **GitHub:** public repositories only, never `facebook/react`. Freeze every moving ref (a branch becomes its SHA)
  in the question or the runner's first step. Confirm a GitHub defect on a second public repository.
- **Fixtures:** a capability check that needs mutation (freshness, dirty-checkout protection) uses a disposable
  checkout it owns: record each edit, restore it, delete only what the check created.

## Run, collect, clean

1. **Serialize timing windows**; honor provider rate limits and record throttling and shared-token contention. Do
   not infer a safe concurrency count.
2. **Capture the transcript per agent:** every call written, every result read, the final answer, wall time and, when
   the host reports them, tokens and cost. Unknown stays unknown, never zero.
3. **Note running processes first** (`octocode serve`, language servers); afterwards stop only the new ones, by PID,
   never by name pattern.
4. **Keep raw evidence** in `campaigns/<version>-<date>-<purpose>/` (local, gitignored) and the conclusion in
   `results/rounds/<version>-<date>-<purpose>/` ([METHOD.md](METHOD.md#record)).
5. **Remove your scratch files** (everything you wrote under `.octocode/tmp/<round>/`), the temporary
   `OCTOCODE_HOME` directories and any fixture you created. Never delete a worktree with uncommitted changes.

## When the environment is the problem

| Symptom | Likely cause | Fix |
|---|---|---|
| Agent cites tools not in its arm | host-level tools not denied | tighten the allowlist; discard the run |
| Octocode answers differ from the served schema | old MCP server still running | restart it from the pinned binary |
| First call slow, repeats fast | cold start versus warm cache | label both; compare like with like |
| Agent quotes a rubric or another arm's wording | leaked context | invalidate; re-run with a clean packet |
| `unauthorized` or empty GitHub results | no token in the fresh home | set `GH_TOKEN` or log in; rerun the preflight |
| Language-server answers differ between arms | server installed for one run only | freeze availability for both arms and report fallbacks |
