A file is a module if the parser sets `SourceFile.ExternalModuleIndicator` to a non-nil node. Otherwise it is a script. `moduleDetection` only changes which extra rules feed into that decision.

**1. Choosing the mode** (`tsc/internal/core/compileroptions.go:243-252`, `GetEmitModuleDetectionKind`)
- An explicit `moduleDetection` value (`auto`, `legacy` or `force`) is used as given. The keys are mapped at `tsoptions/enummaps.go:189-191`.
- If it is unset and `module` is node16, node18, node20 or nodenext, the result is `Force`. I only saw the range check `Node16 <= moduleKind <= NodeNext`, so I didn't confirm which module kinds sit in that range.
- Otherwise the result is `Auto`.

**2. Turning the mode into parse options** (`tsc/internal/ast/parseoptions.go:19-46`, `GetExternalModuleIndicatorOptions`)
- Declaration files get empty options, so `.d.ts` files never get the extra rules (line 20).
- `Force` gives `{Force: true}`.
- `Legacy` gives empty options.
- `Auto` gives `JSX` = true when `jsx` is `react-jsx` or `react-jsxdev`. It gives `Force` = true when `isFileForcedToBeModuleByFormat` returns true (lines 31-37).
- `isFileForcedToBeModuleByFormat` (lines 50-58) returns true if either of these holds:
  - The implied node format is ESNext. For example, a `.js` file in a `type: module` package scope.
  - The extension is `.cjs`, `.cts`, `.mjs` or `.mts`.
- `compiler/fileloader.go:422` calls this function to build the options, and `parser/parser.go:486` passes them to `ast.SetExternalModuleIndicator`.

**3. Deciding module or script** (`ast/parseoptions.go:60-84`, `getExternalModuleIndicator`). Checks run in this order, and the first match wins:
1. JSON files are never modules (they return nil).
2. `isFileProbablyExternalModule` (lines 86-92) applies to every mode. It finds the first top-level statement that is one of these:
   - an `export` modifier
   - `import x = require()`
   - an import declaration
   - an export assignment
   - an export declaration
   
   If none is found, it looks for `import.meta`. That search is guarded by the `PossiblyContainsImportMeta` node flag.
3. Declaration files return nil at this point, so they need an explicit import or export.
4. If `opts.JSX` is set, any JSX element or fragment makes the file a module (`isFileModuleFromUsingJSXTag`).
5. If `opts.Force` is set, the file node itself becomes the indicator, so the file is a module.
6. Otherwise the file is a script.

**Effect of each mode**
- `legacy` uses only step 2, so a file is a module only if it has imports, exports or `import.meta`.
- `auto` adds the JSX rule and the format and extension rule.
- `force` makes every non-declaration, non-JSON file a module.

Later stages compare the indicator with the file node to tell a forced module from one with a real import or export. Examples are `binder/binder.go:936` and `checker/checker.go:15114`.

I read this code but didn't run anything, and I didn't check the test files.