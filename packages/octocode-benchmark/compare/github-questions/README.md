# GitHub questions (shared)

The **one** canonical v2 set of 30 GitHub research questions. All three GitHub matchups run this exact set — they differ only in the baseline arm, never the questions. Published 17-question campaigns used v1 and are not directly comparable to a v2 run.

- [`octocode-vs-gh`](../octocode-vs-gh/) — plain `gh`
- [`octocode-vs-gh-rtk`](../octocode-vs-gh-rtk/) — `gh` + `rtk`
- [`octocode-vs-gh-headroom`](../octocode-vs-gh-headroom/) — `gh` + Headroom

Edit a question **here, once**, and every matchup sees it. A corpus-local matchup
that uses a pinned local checkout keeps its own `questions/` because those
questions are corpus-specific.

Each file contains only the title, ID, and question.

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

## Add a GitHub question

Create `Q<n>.md` (next number) with **only** a title, an `id`, and the `## Question` — no scope, hints, claims, or answer. Then add its row above. It applies to all three GitHub matchups at once.

```markdown
# Q<n> — Short title

**id:** `unique-kebab-id`

## Question

Self-contained, objectively-checkable prompt naming the repo(s)/ref(s) and
exactly what to report.
```
