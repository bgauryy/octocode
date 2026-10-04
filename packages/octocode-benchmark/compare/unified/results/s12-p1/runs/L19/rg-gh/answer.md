Paths below are relative to `tsc/internal/` in the checkout. I didn't read the body of `isFileProbablyExternalModule` past its first lines.

**Answer:** `moduleDetection` is turned into a pair of parse-time flags, `{JSX, Force}`. The parser then uses them, together with a scan of the file's statements, to decide whether the file gets an `ExternalModuleIndicator`. A file with that indicator is a module. A file without it is a script.

**1. Resolving the option** (`core/compilerOptions.go:243-252`, as `GetEmitModuleDetectionKind`)
- If `moduleDetection` is set explicitly, that value is used. The accepted values are `auto`, `legacy` and `force` (`tsoptions/enummaps.go:189-191`).
- If it is unset, the default depends on the emit module kind. `node16` through `nodenext` give `Force`. Everything else gives `Auto`.

**2. Turning the kind into flags** (`ast/parseoptions.go:19-42`, `GetExternalModuleIndicatorOptions`)
- Declaration files (`.d.ts`) get empty options, so no forcing and no JSX rule (`:20-22`).
- `Force` sets `Force: true` (`:25-27`).
- `Legacy` gives empty options, so only the statement scan applies (`:28-30`).
- `Auto` sets two things (`:31-38`):
  - `JSX` is true when `jsx` is `react-jsx` or `react-jsxdev`.
  - `Force` is true when `isFileForcedToBeModuleByFormat` returns true (`:46-54`). That happens when the file's implied node format is ESNext, or the extension is `.cjs`, `.cts`, `.mjs` or `.mts`. A plain `.js` file outside a `type: module` scope is not forced.

**3. Setting the indicator** (`ast/parseoptions.go:56-86`)
- `parser/parser.go:486` calls `SetExternalModuleIndicator`, which runs `getExternalModuleIndicator`. That function checks, in order:
  1. JSON files are never modules.
  2. `isFileProbablyExternalModule` scans the top-level statements for import or export, or `import.meta`. If it finds one, the file is a module. This applies in every mode, including to `.d.ts` files.
  3. Declaration files stop here and are not modules.
  4. If `opts.JSX` is set, a JSX tag in the file makes it a module (`isFileModuleFromUsingJSXTag`).
  5. If `opts.Force` is set, the file is a module, with the source file node itself as the indicator.
  6. Otherwise the file is a script.

**Uncertainty:** I didn't open `isAnExternalModuleIndicatorNode` or `isFileModuleFromUsingJSXTag`. The exact statement kinds they match, and the `import.meta` detail, come from the code comments and not from reading those functions.