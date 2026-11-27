# Agent coordination contract

`contract.json` records the reservation (path lease) rules implemented by
`packages/octocode-agents-communication/src/runtime/paths.ts`. The suite checks the runtime functions and public
commands against the same vectors:

| Suite | What it calls |
|---|---|
| `packages/octocode-agents-communication/tests/coordination-contract.test.ts` | `paths` and `unicode16` from `dist/runtime.js`, and the real `lock` / `lock_many` commands |

## Vectors

| Key | Rule |
|---|---|
| `fold` | One path component is compared as NFD, Unicode full case folding, NFD again. `Straße.ts` and `STRASSE.ts` name one file; so do a Greek final sigma and a capital sigma, `ﬁ` and `fi`, a composed and a decomposed accent. Turkish `İ` folds to `i` + U+0307, and dotless `ı` is its own letter. |
| `keys` | A path's key is `/` plus its folded components (the workspace root is `''`; `.` and empty components are not components). |
| `overlap` | Two reservations conflict when their keys are equal, or when one is a tree whose key is an ancestor of the other's on a component boundary. A file covers only its own key. The relation is symmetric. |
| `batches` | One call that reserves several paths is refused, and takes nothing, if two of its requests overlap each other (a tree and a path inside it, two spellings of one path, one path twice). |

Paths in the vectors are workspace-relative and use `/`.

## Decisions

- **Overlapping requests in one call are refused.** `lock_many` rejects the batch and names the conflicting pair.
  Request the tree alone, or each path once.
- **The case-fold table is frozen.** The package uses its Unicode 16 data (`assets/unicode16.json`), so keys do not
  depend on the Node or ICU version. JavaScript `toLowerCase` keeps `ß` and `ς` apart from `ss` and `σ`, and leaves
  `ﬁ` unchanged; full case folding handles these spellings.

## Changing the fixture

Edit `contract.json` only to record a rule change, and update the runtime and its tests in the same change.
Expected values come from the package's functions: regenerate a vector from `communication.paths`.
