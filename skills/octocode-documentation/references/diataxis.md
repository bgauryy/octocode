# Diátaxis

Load when writing or reviewing human-facing docs. Framework: [Diátaxis](https://diataxis.fr/). One type per page; cross-link the others.

## Choose type

| Signal | Type | Job |
|--------|------|-----|
| New to X; first success; walk me through | Tutorial | Learn by doing |
| How do I…; known task | How-to | Solve a problem |
| Params, endpoints, flags, schema | Reference | Look up facts |
| Why; trade-offs; how it works | Explanation | Understand |

## Patterns

- Tutorial — verb title; Goal → Prerequisites → steps with visible results → outcome. Minimal theory.
- How-to — task title; Goal → assumptions → steps → expected result. Skip essays.
- Reference — name the thing; consistent entries (name, meaning, defaults, links). Lookup in seconds.
- Explanation — concept title; Context → idea → alternatives → perspective. No procedure dumps.

## Separation

- Keep one type per page; link sibling types (tutorial → reference; how-to → explanation).
- Avoid API tables mid-tutorial and narrative essays labeled as reference.
- Before WRITE, also read `references/agent-readable.md` for cross-refs and no-dump rules.

## Outline stub

Type, audience, goal, sections, out path, exclude (what belongs elsewhere).
