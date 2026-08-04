# GitHub questions (shared)

The **one** canonical set of 17 GitHub research questions. All three GitHub matchups run this exact set — they differ only in the baseline arm, never the questions:

- [`octocode-vs-gh`](../octocode-vs-gh/) — plain `gh`
- [`octocode-vs-gh-rtk`](../octocode-vs-gh-rtk/) — `gh` + `rtk`
- [`octocode-vs-gh-headroom`](../octocode-vs-gh-headroom/) — `gh` + Headroom

Edit a question **here, once**, and every matchup sees it. (A corpus-local matchup — one pinned to a local checkout — would keep its own `questions/`, since its questions are corpus-specific.)

Each file is just the title, id, and the question.

| File | Title |
|---|---|
| [Q1.md](Q1.md) | Route regex builder |
| [Q2.md](Q2.md) | Repository discovery and bounded absence |
| [Q3.md](Q3.md) | Flask route history |
| [Q4.md](Q4.md) | Zustand fix PR state |
| [Q5.md](Q5.md) | Vue hydration diff review |
| [Q6.md](Q6.md) | Express router cross-repository trace |
| [Q7.md](Q7.md) | Zustand's Next.js integration contract |
| [Q8.md](Q8.md) | VS Code keybinding dispatch |
| [Q9.md](Q9.md) | Fastify lifecycle contract |
| [Q10.md](Q10.md) | Axios repository and Node entry chain |
| [Q11.md](Q11.md) | Esbuild repository and Node runtime boundary |
| [Q12.md](Q12.md) | Stream and EventEmitter wiring |
| [Q13.md](Q13.md) | Redis security issue and fix PR |
| [Q14.md](Q14.md) | Vitest’s dependency on Vite |
| [Q15.md](Q15.md) | Hono JSX array component PR |
| [Q16.md](Q16.md) | ESLint parser dependency chain |
| [Q17.md](Q17.md) | Next.js fetch request memoization |

## Add a GitHub question

Create `Q<n>.md` (next number) with **only** a title, an `id`, and the `## Question` — no scope, hints, claims, or answer. Then add its row above. It applies to all three GitHub matchups at once.

```markdown
# Q<n> — Short title

**id:** `unique-kebab-id`

## Question

Self-contained, objectively-checkable prompt naming the repo(s)/ref(s) and
exactly what to report.
```
