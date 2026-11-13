# GitHub questions (shared)

The **one** canonical v2 set of 30 GitHub research questions. Every GitHub arm runs this exact set; arms differ only in their tools, never the questions. Published 17-question campaigns used v1 and are not directly comparable to a v2 run.

The arms are in [briefs/arms](../../briefs/arms/) and the method in [METHOD.md](../../METHOD.md).

Edit a question **here, once**, and every arm sees it. Local questions live in the sibling `local-*` sets.

Each file is just the title, id, and the question.

| File | Title |
|---|---|
| [Q1.md](Q1.md) | Next.js route-regex result |
| [Q2.md](Q2.md) | Repository discovery and bounded absence |
| [Q3.md](Q3.md) | Flask route history |
| [Q4.md](Q4.md) | Axios redirect implementation across repositories |
| [Q5.md](Q5.md) | Vue hydration diff review |
| [Q6.md](Q6.md) | Express router cross-repository trace |
| [Q7.md](Q7.md) | Zustand's Next.js integration contract |
| [Q8.md](Q8.md) | VS Code keybinding dispatch |
| [Q9.md](Q9.md) | Fastify lifecycle contract |
| [Q10.md](Q10.md) | Axios repository and Node entry chain |
| [Q11.md](Q11.md) | Esbuild JavaScript-to-Go service boundary |
| [Q12.md](Q12.md) | Stream and EventEmitter wiring |
| [Q13.md](Q13.md) | Redis security issue and fix PR |
| [Q14.md](Q14.md) | Vitest’s dependency on Vite |
| [Q15.md](Q15.md) | Hono JSX array component PR |
| [Q16.md](Q16.md) | ESLint parser dependency chain |
| [Q17.md](Q17.md) | Next.js fetch request memoization |
| [Q18.md](Q18.md) | Vite dependency-section membership |
| [Q19.md](Q19.md) | Node child-process async and sync paths |
| [Q20.md](Q20.md) | Actions toolkit exec output path |
| [Q21.md](Q21.md) | LangChain createAgent flow and graph |
| [Q22.md](Q22.md) | Axios release-range compare |
| [Q23.md](Q23.md) | Linux write() syscall to VFS dispatch flow |
| [Q24.md](Q24.md) | Axios buildFullPath blast radius |
| [Q25.md](Q25.md) | Axios PR selected-patch and review thread |
| [Q26.md](Q26.md) | Zustand documentation surface |
| [Q27.md](Q27.md) | React Query public API surface |
| [Q28.md](Q28.md) | Hermes agent memory management |
| [Q29.md](Q29.md) | MCP HTTP transport authorization flow |
| [Q30.md](Q30.md) | Chromium inter-process communication |

## Repositories the questions use

All public, read live through GitHub at their moving default branch unless a ref is named. Nothing needs
cloning for a research round (an Octocode arm may clone when that is its leanest path). Because branches move,
each answer freezes its SHA and the judge checks at that SHA; a stored answer stays valid at the SHA it cites.

| Repository | Questions | Ref or item named |
|---|---|---|
| `vercel/next.js` | Q1, Q7, Q17 | `canary` (Q1) |
| `sindresorhus/is` | Q2 | discovered by owner and package name |
| `pallets/flask` | Q3 | commit `705e5268` |
| `axios/axios` | Q4, Q10, Q22, Q24, Q25 | tags `v1.18.0`…`v1.19.0` (Q22); PR #10644 (Q25) |
| `follow-redirects/follow-redirects` | Q4 | default branch |
| `vuejs/core` | Q5 | PR #15035 |
| `expressjs/express` | Q6 | default branch; leads to its router dependency's repository |
| `pmndrs/zustand` | Q7, Q26 | default branch |
| `microsoft/vscode` | Q8 | default branch |
| `fastify/fastify` | Q9 | default branch |
| `evanw/esbuild` | Q11 | default branch |
| `nodejs/node` | Q12, Q19 | default branch |
| `redis/redis` | Q13 | an issue and its fix PR, found by search |
| `vitest-dev/vitest` | Q14 | `packages/vitest/package.json` |
| `honojs/hono` | Q15 | PR #5179 |
| `eslint/eslint` | Q16 | default branch; leads to its parser packages' repositories |
| `vitejs/vite` | Q18 | `packages/vite/package.json` |
| `actions/toolkit` | Q20 | default branch |
| `langchain-ai/langchainjs` | Q21 | `libs/langchain/src/agents/index.ts` |
| `torvalds/linux` | Q23 | default branch |
| `TanStack/query` | Q27 | `@tanstack/react-query` package |
| `NousResearch/hermes-agent` | Q28 | default branch |
| `modelcontextprotocol/modelcontextprotocol` | Q29 | specification files |
| `chromium/chromium` | Q30 | default branch |

## Add a GitHub question

Create `Q<n>.md` (next number) with **only** a title, an `id`, and the `## Question` — no scope, hints, claims, or answer. Then add its row above. It applies to every GitHub arm at once; stored baselines have no answer for it, so a new question is compared only among arms run in the same round.

```markdown
# Q<n> — Short title

**id:** `unique-kebab-id`

## Question

Self-contained, objectively-checkable prompt naming the repo(s)/ref(s) and
exactly what to report.
```
