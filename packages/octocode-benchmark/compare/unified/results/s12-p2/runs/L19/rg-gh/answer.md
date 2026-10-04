**Short answer:** `moduleDetection` doesn't classify files directly. It sets two flags, `JSX` and `Force`, that the parser uses when it sets `SourceFile.ExternalModuleIndicator`. A file is a module if that field is non-nil. All paths below are under `tsc/internal/`. I read the code but did not run it, and I did not confirm the git SHA because `git` failed in this sandbox.

**1. Resolving the effective kind** (`core/compileroptions.go:243-251`, `GetEmitModuleDetectionKind`)
- If `moduleDetection` is set explicitly, that value is used. The values are `auto`, `legacy` and `force`, defined at `core/compileroptions.go:379-385` and mapped from strings at `tsoptions/enummaps.go:189-191`.
- Otherwise, if the emit module kind is `node16` through `nodenext`, the kind is `Force`.
- In every other case the kind is `Auto`.

**2. Turning the kind into options** (`ast/parseoptions.go:19-43`, `GetExternalModuleIndicatorOptions`)
- Declaration files (`.d.ts`) get empty options, so only the syntactic check in step 3 applies to them (`:20-22`).
- `Force` sets `Force: true`, so every non-declaration file is a module (`:24-27`).
- `Legacy` sets nothing, so only syntax decides (`:28-30`).
- `Auto` sets two flags (`:31-38`):
  - `JSX` is true when `jsx` is `react-jsx` or `react-jsxdev`.
  - `Force` comes from `isFileForcedToBeModuleByFormat` (`:46-54`). That returns true if the implied node format is ESNext, or the extension is `.cjs`, `.cts`, `.mjs` or `.mts`. A `.js` file outside a `type: module` scope is therefore not forced.

**3. Setting the indicator** (`ast/parseoptions.go:56-84`, `getExternalModuleIndicator`; called from `parser/parser.go:486`)
The checks run in this order, and the first match wins:
1. JSON files are never modules (`:61-63`).
2. `isFileProbablyExternalModule` (`:86-92`) looks for a top-level statement that is an `export` modifier, an `import x = require()`, an import declaration, an export assignment or an export declaration (`:95-98`). Failing that, it looks for `import.meta` (`:101-106`). If it finds one, the file is a module. This check runs for every kind, including `Legacy` and declaration files.
3. Declaration files stop here and are scripts (`:69-71`).
4. If `JSX` is set, a JSX tag makes the file a module (`:73-77`, `isFileModuleFromUsingJSXTag`).
5. If `Force` is set, the file is a module (`:79-81`).
6. Otherwise the file is a script.

**Other places that use the kind:** `tsoptions/showconfig.go:38` and `ls/autoimport/fix.go:915`. `execute/tsc/init.go:199` writes `moduleDetection: force` into the config that `tsc --init` generates.

**Uncertainty:** I did not trace how the `metadata` argument (the implied node format) is computed. That is the input to the `.js` and `type: module` case.