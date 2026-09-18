# Pre-v4 public-suite oracle audit and current gate

The superseded v3 defect table records the evidence behind the v4 correction. It does not
describe the current files. This audit used the locked
LangChain commit `67ee6cb63dd9ae7f3a4dfedc3095652bce15a125` and Next.js commit
`d155ba9ebfffe4742efefda8d68c2e0e8e490924`.

These results are correction evidence, not a replacement for a versioned, independently
materialized oracle set. Changing the public suite requires a new suite version, a new
fixture digest, and new corpus and campaign receipts.

## Current v4 status

Suite v4 corrects the structural and codemod cases below and has a deterministic receipt for
16 of 20 public cases. Cases p09–p12 remain typed `tool-unavailable` gaps because the frozen
Pyright and TypeScript language servers are absent on this host. The public receipt therefore
remains `incomplete`, and the independently curated private manifest remains unsealed.

## Confirmed defective oracles

| Case | Audit result | Required correction |
|---|---:|---|
| p05 | 33 matches across 23 files | Match annotated `ainvoke(self, ...)` async methods; the checked-in pattern returns zero. |
| p06 | 444 assignments | Match annotated and unannotated direct `Field(...)` assignments inside class bodies, stopping at function boundaries. |
| p07 | 353 declarations: 330 `.ts`, 23 `.tsx` | Use structural containment in an export statement and both TypeScript and TSX grammars. This literal reading includes named and default exports. |
| p08 | 3 matches | Use the TSX grammar and support generic `createContext<T>(...)` calls. |
| p16 | 0 matches; empty patch | Mark it explicitly as an expected-empty negative control or replace it with a nonempty codemod case. |

Audit result digests, using a sorted compact JSON row array followed by one newline:

- p05: `2760ccd90dcc47b707c67f071673a3b6266aa78fcefe03416f55bdad899f99cb`
- p06: `ddb81f3b3952c106bd171ff1fff2474a69fa71a7dff3e60054300a42f421bb3e`
- p07: `285c2761f06143cb21c71cfdfeff0abbc610e2e10267f1accd6b977474d80a13`
- p08: `f86ae23ef81a9f8a631471b279e21b06953e4de0e9e61342992d2063e0f292db`
- p16 empty patch: `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`

The suite must formally freeze the normalization algorithm before using these digests as
grading anchors.

## Other underspecified cases

- p02 asks for comparisons or assignments, but its lexical oracle collects every
  `NEXT_RUNTIME` occurrence.
- p03 has prose exclusions but no executable exclusion policy.
- p04 asks for comments, while its lexical oracle also accepts strings and code.
- p09–p12 lack frozen document URIs, UTF-16 positions, server configuration, dependency
  state, and normalized expected ranges.
- p13 lacks a symbol-index schema, language/declaration-kind policy, and exclusions.
- p14 does not structurally prove construction or raising and can collect comments or
  strings.
- p15 lacks a structural definition of environment-variable reads and executable
  production/test exclusions.
- p17 lacks a path constraint, patch and match-identity normalization, and a semantic
  validity guard.
- p18–p20 name subjects but do not contain deterministic expected fact and edge sets.

Every materialized oracle receipt must bind the corpus and suite digests, oracle executable
digest and version, normalization version, row count, answer digest, and generation command.
An independent verifier must reproduce each public result before the first candidate call.

## Gate

Do not start a Terra v3 campaign until the public suite is versioned and rematerialized and
the private suite is independently curated and sealed. Never repair an oracle during a
campaign.
