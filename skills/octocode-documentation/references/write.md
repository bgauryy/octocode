Load when evidence spans files, writing an ADR, or checking API behavior. For a page an agent reads or an agent instruction file, read [agent docs](agent-docs.md).

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

- List the evidence and the gaps for the reader and destination the entry identified; outline a large document or a set before drafting.
- Follow existing terminology and heading style; examples beside their concept; steps in execution order.

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

## Verify

- API or tool docs: trace schema, input, runtime behavior, result; exercise continuations and error examples.
- Separate static types from runtime checks, and measured reliability from untested claims.
