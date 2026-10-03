**Short answer:** In the Go port, `moduleDetection` is resolved to a kind, which becomes two flags (`JSX`, `Force`). Those flags are passed to the parser. After parsing, the parser sets `SourceFile.ExternalModuleIndicator`, and a file counts as a module when that field is non-nil. I read the code at 4f5ddae224 but did not run it. Paths below are relative to `tsc/internal/`.

**1. Resolving the option** (`core/compileroptions.go:243-252`)
- If `moduleDetection` is set, it is used as given. The values are `auto`, `legacy` and `force` (`tsoptions/enummaps.go:189-191`; constants at `core/compileroptions.go:379-385`).
- If it is unset, `module: node16` through `nodenext` gives `force`. Anything else gives `auto`.

**2. Turning the kind into flags** (`ast/parseoptions.go:19-42`, `GetExternalModuleIndicatorOptions`)
- Declaration files (`.d.ts`) get empty options, so neither flag is set (`:20-22`).
- `force` sets `Force: true`, so every non-declaration file is a module (`:25-27`).
- `legacy` sets no flags, so only the syntactic check in step 3 applies (`:28-30`).
- `auto` sets two flags (`:31-38`):
  - `JSX` is true when `jsx` is `react-jsx` or `react-jsxdev`.
  - `Force` is true when `isFileForcedToBeModuleByFormat` returns true (`:46-54`). That happens when the implied node format is ESNext, or the extension is `.cjs`, `.cts`, `.mjs` or `.mts`.

**3. Setting the indicator** (`ast/parseoptions.go:56-84`, `getExternalModuleIndicator`)
The steps run in this order:
1. JSON files are never modules (`:61-63`).
2. `isFileProbablyExternalModule` (`:86-99`) makes the file a module if it has any of:
   - a top-level statement with an `export` modifier;
   - `import x = require(...)`;
   - an import declaration;
   - an export assignment;
   - an export declaration;
   - `import.meta` anywhere in the file, found by a tree walk that is skipped unless the `PossiblyContainsImportMeta` flag is set.
3. Declaration files stop here and return nil (`:69-71`).
4. If `JSX` is set, any JSX element or fragment makes the file a module (`:73-77`; walk at `:132-152`).
5. If `Force` is set, the `SourceFile` node itself is the indicator (`:79-81`).
6. Otherwise the file is a script.

The parser calls `ast.SetExternalModuleIndicator(result, p.opts.ExternalModuleIndicatorOptions)` at `parser/parser.go:486`. `ast.IsExternalModule` is `ExternalModuleIndicator != nil` (`ast/utilities.go:1687`). `IsExternalOrCommonJSModule` (`:1690-1692`) also accepts `CommonJSModuleIndicator`.

**Other uses of the resolved kind:**
- `ls/autoimport/fix.go:915` compares it with `force`.
- `tsoptions/showconfig.go:38` computes it for `--showConfig`.
- `execute/tsc/init.go:199` uses `force` as the default in `--init` output.

**Uncertainty:** `GetImpliedNodeFormatForEmitWorker` decides the ESNext case in `auto` mode. I did not read it, so I can't say how it handles `type: module` package scopes beyond what the code comment says (`ast/parseoptions.go:46-49`).