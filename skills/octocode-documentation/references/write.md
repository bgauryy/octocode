Load when evidence spans files, writing an ADR or agent instruction file, or checking API behavior.

# Write and verify docs

## Gather evidence

- Inventory the root and `<workspace>/docs/`: README, CONTRIBUTING, `AGENTS.md`, ADRs, SECURITY, CI, manifests.
- Find entrypoints, configuration keys, and contracts by path search; read focused slices.
- Durable docs use the repository's supported task entrypoint; verify it against the manifest or task runner before giving a command.
- Spot-check symbols by lexical or structural search; LSP use: `octocode-research`.
- For a complex review, track disputed claims beside their source and verification status; simple edits need no separate claim table. Only verified claims become firm.

| Claim | Evidence (path, module, doc) | Status |
|---|---|---|
| `--dry-run` writes nothing | `src/install.ts`, `install.test.ts` | answered / partial / missing |

## Draft

- Identify the reader, target paths, purpose, evidence, and gaps; outline large documents or sets.
- Follow existing terminology and heading style; examples beside their concept; steps in execution order.

## Agent-readable pages

- One idea per bullet; tables for command and path maps; short pages linking deeper ones; no intros, slogans, or restated README.
- Link the parent index, siblings, and ADRs; never "see docs".
- A command or signature only when the reader must copy it; else link the source. No code dumps (full configuration, long JSON or YAML).
- Links and bullets answer "where do I look?" and "what must I not break?".

| Information | Prefer | Why |
|---|---|---|
| Condition, action and reason | Trigger → action → check table | Agent can select one applicable row. |
| Dependency, branch, state or ordered interaction | Compact Mermaid diagram | Exposes relationships; keep conditions on edges. |
| Copyable command or exact schema | Code fence with prerequisites and expected result | Preserves executable syntax. |
| One fact or short explanation | Prose | Avoid a visual that takes longer to parse. |

## ADR

- Only for a settled choice that is expensive to reverse (stack, schema, auth, API style, infra); not obvious code or prototypes.

| Situation | Action |
|---|---|
| Convention found (`docs/adr`, `docs/decisions`, `.adr-dir`, MADR), consistent | Match its location, numbering, and headings |
| Convention found, conflicting | Surface the conflict; no second scheme |
| No convention | Numbered ADR under `docs/decisions`, named `ADR-NNN-short-title` (`ADR-007-use-postgres.md`) |

- Decision (ADR) sections: Status (proposed, accepted, superseded, or deprecated), Date, Context, Decision, Alternatives considered, Consequences. Keep Status accurate; supersede, never delete.
- Link it from the relevant index or architecture doc.
- Definition of Done: convention matched or default justified, at least one alternative or an explicit "none", no secrets.

## Agent instruction files

When `octocode-agentic` is available, it owns behavioral wording; this section covers index structure and link checks.

- Check how the target host loads `AGENTS.md` or `CLAUDE.md`; preserve its scope and precedence. Keep nested instructions focused on local differences.
- Draft as an index. Run `node <skill-folder>/scripts/doc-check.mjs <file> --checks links,anchors,commands`: it reports links and anchors that do not resolve and commands whose script or package script is missing. Run or inspect each command it cannot resolve, such as a tool on `PATH`.
- Sections only for non-obvious value: package manager (one line); Commands table (task, command, file-scoped test or lint); External references table (need, path); Key conventions that prevent likely mistakes.
- Symlink between hosts only when both read the same rules and the repository supports it.
- Omit welcome text, skill lists, and linter-config restatements.

## Verify

- API or tool docs: trace schema, input, runtime behavior, result; exercise continuations and error examples.
- Separate static types from runtime checks, and measured reliability from untested claims.
