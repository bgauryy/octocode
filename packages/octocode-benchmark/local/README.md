# Local benchmark: repositories and setup

Everything local runs on pinned clones under `local/repos/` (gitignored). This page lists every repository
the local question sets, the [local flows benchmark](FLOWS.md), the
[graph battery](../../../skills-dev/octocode-graph-dev/references/battery.md) and the grammar checks need,
at the commit they are pinned to. Keep the path: graph-dev reads it too.

## Clone

Shallow-clone each repository at its pinned commit, from `packages/octocode-benchmark/local/repos`. The form,
one line per repository:

```sh
git init -q DIR && git -C DIR remote add origin URL && git -C DIR fetch -q --depth 1 origin COMMIT && git -C DIR checkout -q FETCH_HEAD
```

Reuse existing clones in place; check one with `git -C DIR rev-parse HEAD` against the table. If a pinned
commit can't be fetched, clone the default branch with `--depth 1`, record its HEAD in the round, and treat
answers on that clone as not comparable with stored baselines. Runs reject a dirty tracked tree.

## Repositories

Sets: **core** = [local-core](../questions/local-core/README.md) (30), **goals** =
[local-goals](../questions/local-goals/README.md) (40), **transfer** = local-transfer (12), **scenarios** =
local-scenarios (30). The question files name clones by path; this table owns the commits.

| Dir | Repository | Pinned commit | Language | Used by |
|---|---|---|---|---|
| `asm` | https://github.com/libjpeg-turbo/libjpeg-turbo | `03c9a9d795c08ab06d625e5fc3bf0ae8b26f9652` | Assembly | scenarios; grammar |
| `c` | https://github.com/redis/redis | `20bb2cfc54aa08c8fdfb8c4c0a8b8258e811711e` | C | core, goals, transfer, scenarios; grammar |
| `cpp` | https://github.com/nlohmann/json | `f422b753cca0143b2c8545df2e1422171a2f9cd5` | C++ | core, transfer, scenarios; grammar |
| `csharp` | https://github.com/JamesNK/Newtonsoft.Json | `52fa3aef1f2cadcd3a3f874251eddc98d3efbbaa` | C# | core, goals, transfer, scenarios; grammar |
| `cuda` | https://github.com/NVIDIA/cutlass | `78e46228a1bfc5c972e2f9a634223af784cd6949` | CUDA | grammar (optional) |
| `go` | https://github.com/prometheus/prometheus | `ea954809ceafceb53ecfa295ab0947753929de9e` | Go | core, goals, transfer, scenarios; grammar |
| `java` | https://github.com/google/guava | `4d41665af1b05bad09039b82cb2a1a474a7bceaf` | Java | core, goals, transfer, scenarios; grammar |
| `javascript` | https://github.com/lodash/lodash | `2b5e6f7399a7b48005140b5d5c6bc6c0e62919a8` | JavaScript | core, goals, transfer, scenarios; grammar |
| `langchain` | https://github.com/langchain-ai/langchain | `67ee6cb63dd9ae7f3a4dfedc3095652bce15a125` | Python | core, scenarios |
| `nextjs` | https://github.com/vercel/next.js | `d155ba9ebfffe4742efefda8d68c2e0e8e490924` | JavaScript (CommonJS) | core, scenarios; grammar |
| `python` | https://github.com/django/django | `4fab678a0739d54401ccee7eb587553657c9f76e` | Python | core, goals, transfer, scenarios; grammar |
| `rust` | https://github.com/tokio-rs/tokio | `facc6fc47eb2ba13a465a3179f8e34db620561ec` | Rust | core, goals, transfer, scenarios; grammar |
| `scala` | https://github.com/typelevel/cats | `4a2ea736632c64aaf2e4915e6922e3585379382f` | Scala | core, scenarios; grammar |
| `terraform` | https://github.com/hashicorp/terraform (tag `v1.14.7`) | `c0fcd5b52b08f607fbd1e2f164b323f7523146c9` | Go, HCL | goals (Q33–Q40), transfer, scenarios |
| `tsx` | https://github.com/excalidraw/excalidraw | `f3d99c42e905706c1899bd887cfec59f2e92f425` | TSX | core, goals, transfer, scenarios; grammar |
| `typescript` | https://github.com/microsoft/TypeScript | `4f5ddae224b9529a69b571f631fe3a2cb283162f` | TypeScript (+ Go in the TS 7 port) | core, scenarios; grammar |
| `huge-ts` | https://github.com/microsoft/vscode | `43dd9070f75d527ab38035c0562acbfe9de4209b` | TypeScript | core, scenarios; large-repo flows |
| `huge-go` | https://github.com/kubernetes/kubernetes | `6c1c7702cf2052245ef10e699d45f071af306f59` | Go | core, scenarios; large-repo flows |
| `huge-java` | https://github.com/elastic/elasticsearch | `9438d0a669470822054ce2f2278b1613d07a678e` | Java | large-repo flows (optional) |
| `huge-rust` | https://github.com/rust-lang/rust | `b373574eea1fa546ae5a757d07a8a3ea207c0fe5` | Rust | large-repo flows (optional) |
| `huge-cpp` | https://github.com/pytorch/pytorch | `03943c7f9ea8c146348e5a12f9fc75e32ee21fdc` | C++ | large-repo flows (optional) |
| `huge-c` | https://github.com/torvalds/linux | `fd179f8a05be3ccae366b9b96e176b51fbe54aab` | C | large-repo flows (optional) |

For one set only, clone its rows; for a local research round on local-core, clone the 15 core rows. Before a run, check
`git -C DIR rev-parse HEAD` and `git -C DIR status --short`: a different commit or a dirty tracked tree makes
answers not comparable with stored ones.

## Language notes

- **Grammar fixtures:** `typescript`, `tsx`, `javascript`, `python`, `go`, `rust`, `java`, `c`, `cpp`,
  `csharp`, `scala`, `asm` and `cuda`; `nextjs` adds CommonJS. Checking every shipped grammar needs Assembly
  and CUDA too. A grammar does not guarantee an installed language server: missing servers show as
  incomplete coverage, not as a pass.
- **`cuda`** has no compile database. Without one, `lspSearch` runs clangd with CUDA host-only fallback flags
  (`-xcuda --cuda-host-only`), so `.cu` and `.cuh` files get an AST. Without a CUDA SDK, CUDA builtins
  (`__global__`, `blockIdx`) stay unresolved and project headers resolve only relative to the including file.
  For full semantics, add a `compile_commands.json` or `compile_flags.txt` to `cuda/`: lspSearch restarts an
  idle clangd when one appears or changes (see `packages/octocode-native/docs/engine/LSP_SERVER_LIFECYCLE.md`).

## Tools to install for the comparisons

`rg`, `ast-grep` (`sg`), `fd`, `git`; graphify for the graph comparison (owner installs it). Check versions at
the start of a round ([COMPARISONS.md](../COMPARISONS.md#map) lists the last known ones).
