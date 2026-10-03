I read the option-handling code in `tsc/internal/ast/parseoptions.go` and `tsc/internal/core/compileroptions.go`, and here is how it decides.

**Answer:** The parser gets a `ExternalModuleIndicatorOptions{JSX, Force}` value chosen from the effective `moduleDetection` kind. It then sets `file.ExternalModuleIndicator`. A non-nil indicator means the file is a module. Otherwise it is a script.

**1. Effective kind** (`core/compileroptions.go:243-251`, `GetEmitModuleDetectionKind`)
- An explicit `moduleDetection` value (not `ModuleDetectionKindNone`) is used as given.
- If unset and `module` is between `Node16` and `NodeNext`, the kind is `Force`.
- Otherwise the kind is `Auto`.
- The enum values are `None=0`, `Auto=1`, `Legacy=2` and `Force=3` (`compileroptions.go:379-385`).

**2. Options per file** (`ast/parseoptions.go:19-`, `GetExternalModuleIndicatorOptions`)
- Declaration files (`.d.ts` and similar) get empty options. They are modules only if they contain import or export syntax.
- `Force`: sets `Force: true`, so every non-declaration file is a module.
- `Legacy`: returns empty options. A file is a module only if it has imports, exports or `import.meta`.
- `Auto`: sets two things:
  - `JSX` is true when `jsx` is `react-jsx` or `react-jsxdev`.
  - `Force` comes from `isFileForcedToBeModuleByFormat`. That is true when the implied node format for the file is ESNext. It is also true for `.cjs`, `.cts`, `.mjs` and `.mts` files. A `.js` file outside a `type: module` scope is not forced.

**3. Final decision** (`ast/parseoptions.go:56-84`, `SetExternalModuleIndicator` and `getExternalModuleIndicator`), checked in this order:
1. JSON files are never modules.
2. `isFileProbablyExternalModule` (line 86) returns a module indicator if any top-level statement:
   - has an `export` modifier,
   - is `import x = require(...)`,
   - is an import declaration,
   - is an export assignment, or
   - is an export declaration.

   It also returns one if the file contains `import.meta`, which is found by walking the tree only when `NodeFlagsPossiblyContainsImportMeta` is set.
3. Declaration files stop here and are scripts if nothing matched.
4. If `opts.JSX` is set, a JSX tag anywhere in the file makes it a module.
5. If `opts.Force` is set, the file node itself becomes the indicator, so the file is a module.
6. Otherwise the file is a script.

**Uncertainty:** I did not read where `GetExternalModuleIndicatorOptions` is called, or the JSX-tag walker `walkTreeForJSXTags`. Some line numbers are approximate. Those for `isFileForcedToBeModuleByFormat` and the end of the file come from reading the returned text, not from a line-numbered search.