# Local questions (corpus-local)

30 research questions against the pinned clones in
[`packages/octocode-benchmark/local/repos/`](../../../../../packages/octocode-benchmark/local/repos/README.md)
(absolute paths under `packages/octocode-benchmark/local/repos`). Arms whose
name starts with `local-` (`local-shell`, `local-mcp`, `local-mcp-*`) run this set; every other
arm runs [`../github-questions/`](../../../external/questions/README.md).

Each file is the title, an `id` and the `## Question`: self-contained, naming the clone, and
objectively checkable at the pinned commit. The ground truth for each question lives in
[`answers/`](answers) — runners never see it; the judge reads it and re-verifies it with
read-only shell commands on the clone.

| File | Kind | Clone | Title |
|---|---|---|---|
| [Q1.md](Q1.md) | basic: definition | rust (tokio) | Tokio's free spawn_blocking function |
| [Q2.md](Q2.md) | basic: read a body | javascript (lodash) | Lodash chunk body |
| [Q3.md](Q3.md) | basic: list a directory | go (prometheus) | Prometheus chunk encoding directory |
| [Q4.md](Q4.md) | basic: definition | c (redis) | Redis listpack encoding constant |
| [Q5.md](Q5.md) | basic: definition | scala (cats) | Cats Functor type class |
| [Q6.md](Q6.md) | basic: definition + body | python (django) | Django get_random_string |
| [Q7.md](Q7.md) | advanced: call sites | tsx (excalidraw) | Excalidraw isTextBindableContainer call sites |
| [Q8.md](Q8.md) | advanced: interface + callers | go (prometheus) | Prometheus NewRangeQuery callers |
| [Q9.md](Q9.md) | advanced: AST shape | rust (tokio) | Tokio fs functions that skip asyncify |
| [Q10.md](Q10.md) | advanced: subclasses across files | python (django) | Django SuspiciousOperation subclasses |
| [Q11.md](Q11.md) | advanced: outline of a large file | python (django) | Django query.py outline |
| [Q12.md](Q12.md) | advanced: package.json membership | tsx (excalidraw) | Excalidraw package dependency sections |
| [Q13.md](Q13.md) | advanced: Cargo.toml membership | rust (tokio) | Tokio backtrace dependency |
| [Q14.md](Q14.md) | advanced: type hierarchy | java (guava) | Guava ForwardingCollection subclasses |
| [Q15.md](Q15.md) | advanced: callers + arguments | c (redis) | Redis setGenericCommand callers |
| [Q16.md](Q16.md) | edge: BOM-prefixed first lines | csharp (Newtonsoft.Json) | Newtonsoft license headers behind a BOM |
| [Q17.md](Q17.md) | edge: 8 MB file, repeated declaration | huge-ts (vscode) | Checker declarations in an 8 MB fixture |
| [Q18.md](Q18.md) | edge: 1.1 MB lockfile, repeated literal | langchain | LangChain v1 langgraph pin and lock |
| [Q19.md](Q19.md) | edge: repeated literal, narrow windows | typescript | TypeScript assignability diagnostic codes |
| [Q20.md](Q20.md) | edge: name collides across languages | rust + go | Histogram in Rust versus Go |
| [Q21.md](Q21.md) | edge: non-ASCII path | python (django) | Django non-ASCII static file |
| [Q22.md](Q22.md) | edge: absent result, scope stated | go (prometheus) | pkg/errors in Prometheus |
| [Q23.md](Q23.md) | workflow: described → search → read → callers | go (prometheus) | Prometheus target relabeling function |
| [Q24.md](Q24.md) | workflow: multi-repo comparison | tsx + nextjs + huge-ts | TypeScript compiler pins across three repos |
| [Q25.md](Q25.md) | workflow: described target, huge repo | huge-go (kubernetes) | Kubelet eviction decisions |
| [Q26.md](Q26.md) | workflow: described target | c (redis) | Sorted-set add option conflicts |
| [Q27.md](Q27.md) | workflow: described target | rust (tokio) | Tokio work-stealing search |
| [Q28.md](Q28.md) | workflow: described target | python (django) | Django CSRF checks on HTTPS |
| [Q29.md](Q29.md) | workflow: described target, 5k-line file | java (guava) | Guava cache weight eviction |
| [Q30.md](Q30.md) | workflow: described target | cpp (nlohmann/json) | JSON lexer surrogate pairs |

"Described" questions (Q23, Q25–Q30) describe behavior instead of naming the symbol or the
literal, the case where a semantic locate (clasify) should be the lean path.
