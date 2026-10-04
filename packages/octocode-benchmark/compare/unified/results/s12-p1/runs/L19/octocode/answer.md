**Answer:** `moduleDetection` doesn't classify files directly. It is resolved to a kind, and that kind is turned into `ExternalModuleIndicatorOptions` (`JSX`, `Force`). Those options are then applied after parsing to set `file.ExternalModuleIndicator`. A file is a module if that indicator is non-nil, and a script otherwise. In this checkout the Go code is under `tsc/internal/`, not `internal/`.

**1. Resolving the kind**: `core/compileroptions.go:243-252`, `GetEmitModuleDetectionKind`
- If `moduleDetection` is set explicitly (`auto`, `legacy` or `force`), that value is used. The enum is at `compileroptions.go:379-385` and the string mapping at `tsoptions/enummaps.go:189-191`.
- If it is unset and the emit module kind is between `Node16` and `NodeNext`, the kind is `Force`.
- If it is unset otherwise, the kind is `Auto`.

**2. Kind to indicator options**: `ast/parseoptions.go:19-42`, `GetExternalModuleIndicatorOptions`
- Declaration files (`.d.ts` and similar) get empty options (line 20), so they are only modules if the normal syntactic check finds an import or export.
- `Force` returns `{Force: true}`, so every non-declaration file is a module.
- `Legacy` returns empty options, so only syntax makes a file a module.
- `Auto` returns two settings:
  - `JSX` is true when `jsx` is `react-jsx` or `react-jsxdev`.
  - `Force` is `isFileForcedToBeModuleByFormat` (lines 46-54), which is true when the implied node format is ESNext or the extension is `.cjs`, `.cts`, `.mjs` or `.mts`. Plain `.js` files outside a `type: module` scope are not forced.

**3. Setting the indicator**: `ast/parseoptions.go:56-84`, `getExternalModuleIndicator`
- JSON files are never modules (line 61).
- The syntactic check `isFileProbablyExternalModule` (lines 86-106) runs next. It looks for these:
  - an `export` modifier;
  - `import x = require()`;
  - an import declaration;
  - an export assignment;
  - an export declaration;
  - `import.meta`.

  If any is found, the file is a module.
- Declaration files stop here and return nil (line 69).
- If `opts.JSX` is set, a full tree walk looks for a JSX element or fragment (lines 73-77, 122-148). Finding one makes the file a module.
- If `opts.Force` is set, the file node itself becomes the indicator (lines 79-81).
- Otherwise the file is a script.

**Uncertainty:** I read only these two files. I did not trace where `GetExternalModuleIndicatorOptions` and `SetExternalModuleIndicator` are called from the parser or program. I also did not check the `Auto` case's use of `GetImpliedNodeFormatForEmitWorker` beyond the call at line 50.