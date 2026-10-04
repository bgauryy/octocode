**Short answer:** `moduleDetection` is resolved to one of `auto`, `legacy` or `force`. That choice is turned into per-file parse options (`JSX` and `Force` flags). The parser then sets `file.ExternalModuleIndicator` from those flags. A file with a non-nil indicator is a module. Otherwise it is a script.

All paths below are under `tsc/internal/`.

**1. Resolving the effective kind** (`core/compileroptions.go:243-252`)
- If `ModuleDetection` is set (`auto`, `legacy` or `force`), that value is used. The option is parsed from `tsoptions/enummaps.go:189-191`.
- If it is unset, the kind is `Force` when `module` is `node16` through `nodenext`.
- In every other unset case, the kind is `Auto`.

**2. Turning the kind into parse options** (`ast/parseoptions.go:19-42`, `GetExternalModuleIndicatorOptions`)
- Declaration files get empty options, so no forcing and no JSX rule (`:20-22`).
- `Force` returns `{Force: true}`. Every non-declaration file is a module (`:25-27`).
- `Legacy` returns empty options. Only the syntactic checks in step 3 apply (`:28-30`).
- `Auto` returns:
  - `JSX` is true when `jsx` is `react-jsx` or `react-jsxdev`.
  - `Force` is `isFileForcedToBeModuleByFormat` (`:31-38`). That function is true if the implied emit format for the file is ESNext, or the extension is `.cjs`, `.cts`, `.mjs` or `.mts` (`:44-54`).
  - The ESNext case covers `.js` files in a `type: module` scope under node16/nodenext.
- `compiler/fileloader.go:422` computes these options, and `parser/parser.go:486` calls `ast.SetExternalModuleIndicator`.

**3. Per-file decision** (`ast/parseoptions.go:60-84`, `getExternalModuleIndicator`). The checks run in this order:
1. JSON files are never modules (`:61-63`).
2. A syntactic indicator makes the file a module (`:65-67`). That means any of the following at top level, or `import.meta` anywhere (`:86-106`):
   - an `export` modifier,
   - `import x = require()`,
   - an import declaration,
   - an export assignment,
   - an export declaration.
3. Declaration files stop here and return nil, so they stay scripts without an explicit import or export (`:69-71`).
4. If `opts.JSX` is set and the file contains a JSX element or fragment, it is a module (`:73-77`, tree walk at `:129-149`).
5. If `opts.Force` is set, the file is a module, and the indicator is the `SourceFile` node itself (`:79-81`).
6. Otherwise it is a script (`:83`).

**Uncertainty:** I did not trace how downstream code (checker, emit) consumes the indicator. I also did not check `contentmapper/transform.go:62,89`, which forces `Force = true` for its own purposes.