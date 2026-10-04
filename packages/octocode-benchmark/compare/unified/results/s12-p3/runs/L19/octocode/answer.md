**Short answer:** `moduleDetection` doesn't classify a file directly. It selects a set of `ExternalModuleIndicatorOptions` (`JSX`, `Force`). A file is a module if its `ExternalModuleIndicator` node is non-nil. Otherwise it is a script. I did not trace where `SetExternalModuleIndicator` is called from the parser or program.

**1. Resolving the effective kind** (`tsc/internal/core/compileroptions.go:243-252`, `GetEmitModuleDetectionKind`)
- If `moduleDetection` is set (not `None`), that value is used.
- If it is unset and `module` is `node16` through `nodenext`, the kind is `Force`.
- Otherwise the kind is `Auto`.
- The enum is `None`, `Auto`, `Legacy`, `Force` (`compileroptions.go:379-385`). The strings `auto`, `legacy` and `force` map to these in `tsoptions/enummaps.go:189-191`.

**2. Mapping kind to indicator options** (`tsc/internal/ast/parseoptions.go:19-42`, `GetExternalModuleIndicatorOptions`)
- Declaration files (`.d.ts` etc.) get empty options, so they rely only on the usual import/export check (line 20).
- `Force`: `{Force: true}`, so every non-declaration file is a module (lines 25-27).
- `Legacy`: empty options, so only imports, exports or `import.meta` make a file a module (lines 28-30).
- `Auto` (lines 31-38):
  - `JSX` is true when `jsx` is `react-jsx` or `react-jsxdev`.
  - `Force` is true when `isFileForcedToBeModuleByFormat` returns true (lines 46-54). That happens when the implied node format is ESNext, or the extension is `.cjs`, `.cts`, `.mjs` or `.mts`.

**3. Setting the indicator on the source file** (`parseoptions.go:56-84`, `SetExternalModuleIndicator` and `getExternalModuleIndicator`)
The checks run in this order:
1. A JSON file is never a module (line 61).
2. If `isFileProbablyExternalModule` finds a node, that node is the indicator (lines 65-67). It looks for:
   - an `export` modifier;
   - an `import x = require()`;
   - an import declaration;
   - an export assignment or export declaration;
   - `import.meta` (lines 86-106).
3. Declaration files stop here and are scripts if nothing matched (line 69).
4. If `opts.JSX` is set, any JSX element or fragment makes the file a module (lines 73-77).
5. If `opts.Force` is set, the file node itself is the indicator, so the file is a module (lines 79-81).
6. Otherwise the file is a script.

**Other uses:** `ls/autoimport/fix.go:915` compares the detection kind against `Force`.

**Uncertainty:** `GetImpliedNodeFormatForEmitWorker` (used at `parseoptions.go:50`) was not read. The claim that `.js` files in a `type: module` scope count as ESM comes from the code comment at lines 46-49, not from that function.