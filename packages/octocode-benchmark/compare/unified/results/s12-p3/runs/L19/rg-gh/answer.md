**Short answer:** The Go compiler resolves `moduleDetection` into a per-file `ExternalModuleIndicatorOptions{JSX, Force}`. The parser then uses those options to set `SourceFile.ExternalModuleIndicator`. A file with a non-nil indicator is a module. A file with a nil indicator is a script. All paths below are under `tsc/internal/`. I read the code but did not run it.

**1. Resolve the effective kind**
- `GetEmitModuleDetectionKind` (`core/compileroptions.go:243-252`) returns the explicit `ModuleDetection` option if it is set.
- Otherwise it returns `Force` when `module` is `node16` through `nodenext`, and `Auto` in all other cases.
- The enum values are `None`, `Auto`, `Legacy` and `Force` (`core/compileroptions.go:379-385`). The strings `auto`, `legacy` and `force` are mapped in `tsoptions/enummaps.go:189-191`.

**2. Turn the kind into per-file options**
- `GetExternalModuleIndicatorOptions` (`ast/parseoptions.go:19-42`) is called from `compiler/fileloader.go:422`.
- Declaration files (`.d.ts` and similar) get empty options, so they use only the syntactic check in step 3.
- `Force` returns `{Force: true}`, so every non-declaration file is a module.
- `Legacy` returns empty options, so only the syntactic check in step 3 applies.
- `Auto` returns `JSX` set when `jsx` is `react-jsx` or `react-jsxdev`. It sets `Force` when `isFileForcedToBeModuleByFormat` is true.
- `isFileForcedToBeModuleByFormat` (`ast/parseoptions.go:46-54`) is true in two cases:
  - the file's implied node format is ESNext, which covers `.js` files in a `type: module` scope under node16/nodenext;
  - the extension is `.cjs`, `.cts`, `.mjs` or `.mts` (`ast/parseoptions.go:44`).

**3. Set the indicator during parsing**
- `parser/parser.go:486` calls `ast.SetExternalModuleIndicator`, which calls `getExternalModuleIndicator` (`ast/parseoptions.go:60-84`). It checks these in order:
  1. JSON files are never modules.
  2. A syntactic indicator (`isFileProbablyExternalModule`, `ast/parseoptions.go:86-92`) makes the file a module. This is any top-level statement that is an `export`-modified declaration, an `import x = require()`, an import declaration, an export assignment or an export declaration (`ast/parseoptions.go:94-98`). An `import.meta` use also counts (`ast/parseoptions.go:100-105`).
  3. Declaration files return nil here, which makes them scripts.
  4. If `opts.JSX` is set, a JSX element or fragment makes the file a module (`ast/parseoptions.go:127` onward).
  5. If `opts.Force` is set, the file node itself is the indicator, so the file is a module.
  6. Otherwise the result is nil and the file is a script.

**Other uses**
- `contentmapper/transform.go:62` and `:89` set `Force = true` directly.
- `ls/autoimport/fix.go:915` checks whether the kind is `Force`.
- `tsc --init` writes `moduleDetection: force` (`execute/tsc/init.go:199`).

**Uncertainty:** I did not view the body of `walkTreeForJSXTags` past its start. I also did not check how `GetImpliedNodeFormatForEmitWorker` computes the ESM format. The commit hash printed nothing because git failed in the sandbox. I took the checkout to be the pinned commit as stated.