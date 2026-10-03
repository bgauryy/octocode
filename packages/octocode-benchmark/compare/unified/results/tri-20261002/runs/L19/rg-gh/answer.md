In the Go port, a file is a module if the parser sets its `ExternalModuleIndicator` to a non-nil node. Otherwise it's a script. `moduleDetection` doesn't set that directly. It only decides which extra rules, beyond the always-on syntactic check, can force a file to be a module. I couldn't run `git rev-parse` because of a sandbox `/dev/null` error, so I didn't confirm the checkout is at 4f5ddae224. All paths are under `tsc/internal/`.

**1. The effective setting** (`core/compileroptions.go:243-251`, `GetEmitModuleDetectionKind`)
- An explicit `moduleDetection` value (`auto`, `legacy` or `force`) is returned as is.
- If it is unset and `module` is `node16` through `nodenext`, the result is `force`.
- Otherwise the result is `auto`.
- The string-to-enum mapping is at `tsoptions/enummaps.go:189-191`.

**2. Per-file parse options** (`ast/parseoptions.go:19-42`, `GetExternalModuleIndicatorOptions`)
- Declaration files get empty options, so no extra rules apply.
- `force` returns `Force: true`.
- `legacy` returns empty options.
- `auto` returns two things:
  - `JSX` is true when `jsx` is `react-jsx` or `react-jsxdev`.
  - `Force` is true when `isFileForcedToBeModuleByFormat` says so (`ast/parseoptions.go:46-54`). That holds if the implied node format is ESNext, or the extension is `.cjs`, `.cts`, `.mjs` or `.mts`.
- `compiler/fileloader.go:422` builds these options when loading a file.

**3. Setting the indicator** (`ast/parseoptions.go:56-84`, `getExternalModuleIndicator`; called from `parser/parser.go:486`)
- JSON files are never modules.
- Next comes `isFileProbablyExternalModule` (`ast/parseoptions.go:87-94`). It makes the file a module if any top-level statement is an indicator node:
  - an `export` modifier,
  - `import x = require()`,
  - an import declaration,
  - an export assignment,
  - an export declaration.

  It also counts `import.meta`, found by a tree walk gated by the `PossiblyContainsImportMeta` flag. This check applies in every mode, and it is the only rule that applies to declaration files.
- Declaration files stop there and return nil if nothing matched.
- If `opts.JSX` is set, any JSX element or fragment makes the file a module (`walkTreeForJSXTags`).
- If `opts.Force` is set, the indicator is the source file node itself.
- Otherwise the file is a script.

**Downstream use**
- The binder and checker test `ExternalModuleIndicator != nil`, for example `binder/binder.go:1346`.
- A file whose indicator is the file node itself was forced rather than detected by syntax. `ls/autoimport/fix.go:894-916` handles that case.

I read these code paths directly. I didn't run any tests.