**Short answer:** `moduleDetection` doesn't classify a file by itself. It picks an `ExternalModuleIndicatorOptions{JSX, Force}` value for each file. `getExternalModuleIndicator` then sets `file.ExternalModuleIndicator` from that value. A non-nil indicator means module, and nil means script. All paths below are under `tsc/internal/`. I read the code but did not run it.

**1. Option values and defaults**
- `core.ModuleDetectionKind` has the values None=0, Auto=1, Legacy=2 and Force=3 (`core/compileroptions.go:379-386`). The user-facing strings `auto`, `legacy` and `force` map to Auto, Legacy and Force in `tsoptions/enummaps.go:189-191`.
- `GetEmitModuleDetectionKind` (`core/compileroptions.go:243-252`) returns the explicit option if it isn't None.
- Otherwise it returns `Force` when the emit module kind is between Node16 and NodeNext inclusive.
- In every other case it returns `Auto`.

**2. Per-file options: `GetExternalModuleIndicatorOptions` (`ast/parseoptions.go:19-42`)**
- Declaration file names (`.d.ts` and similar) return empty options, so no forcing and no JSX rule.
- `Force` returns `{Force: true}`.
- `Legacy` returns empty options.
- `Auto` returns `JSX: Jsx == ReactJSX || ReactJSXDev`, and `Force: isFileForcedToBeModuleByFormat(...)`.
- Any other value returns empty options.
- `isFileForcedToBeModuleByFormat` (`parseoptions.go:46-56`) is true when the implied node format for emit is `ModuleKindESNext`. It is also true for the extensions `.cjs`, `.cts`, `.mjs` and `.mts`. Plain `.js` files outside a `type: module` scope are not forced.

**3. Final decision: `getExternalModuleIndicator` (`ast/parseoptions.go:62-86`)**
Checks run in this order, and the first one that applies decides:
1. JSON files are never modules (nil).
2. If `isFileProbablyExternalModule` finds an indicator, the file is a module. Indicators are `export` modifiers, `import x = require()`, import declarations, export assignments, export declarations, and `import.meta`. This check runs for every mode, including `Legacy`, and for declaration files.
3. Declaration files stop here and return nil. That is why they still need an explicit `export {}` or similar.
4. If `opts.JSX` is set and the file contains a JSX element, opening-like element or fragment, it is a module. In practice this only applies under `Auto` with `react-jsx` or `react-jsxdev`.
5. If `opts.Force` is set, the file node itself becomes the indicator, so it is a module.
6. Otherwise it is nil, so the file is a script.

**In effect**
- **`legacy`:** a file is a module only if it has import/export syntax or `import.meta`.
- **`auto` (the default):** the legacy rules, plus JSX tags under `react-jsx`/`react-jsxdev`, plus ESM-format files and `.cjs`/`.cts`/`.mjs`/`.mts`.
- **`force`:** every non-declaration, non-JSON file is a module.
- **Default under `module: node16`/`nodenext`:** this becomes `force`.

**Uncertainty:** I did not trace where `GetExternalModuleIndicatorOptions` and `SetExternalModuleIndicator` are called from the parser. I also did not check `autoimport/fix.go`, which reads the same option.