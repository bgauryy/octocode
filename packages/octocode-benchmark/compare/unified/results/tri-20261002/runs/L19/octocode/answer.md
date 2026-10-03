The Go compiler turns `moduleDetection` into an `ExternalModuleIndicatorOptions{JSX, Force}` struct. The parser then uses that struct to decide whether a file gets an `ExternalModuleIndicator` node, which is what makes it a module. I did not trace where the parser calls these functions.

**1. Resolving the option.** `GetEmitModuleDetectionKind()` is at `tsc/internal/core/compileroptions.go:243-252`.
- If `moduleDetection` is set (not `None`), that value is used.
- Otherwise, if the emit module kind is `Node16` through `NodeNext`, the result is `Force`.
- Otherwise the result is `Auto`.
- The three kinds are `auto`, `legacy` and `force`, defined at `compileroptions.go:379-385` and mapped from strings at `tsoptions/enummaps.go:189-191`.

**2. Per-file options.** `GetExternalModuleIndicatorOptions` is at `tsc/internal/ast/parseoptions.go:19-42`.
- Declaration files get empty options (`:20-22`). They are modules only if the syntactic check in step 3 finds one.
- `force` returns `{Force: true}` (`:25-27`). Every non-declaration file becomes a module.
- `legacy` returns empty options (`:28-30`). Only the syntactic check applies.
- `auto` (`:31-38`) returns:
  - `JSX: true` when `jsx` is `react-jsx` or `react-jsxdev`.
  - `Force` set by `isFileForcedToBeModuleByFormat` (`:46-54`). That is true for `.cjs`, `.cts`, `.mjs` and `.mts` files, and for files whose implied node format is ESNext. The latter covers `.js` files in a `type: module` scope under node16/nodenext.

**3. Deciding module or script.** `getExternalModuleIndicator` is at `parseoptions.go:60-84`. It checks in this order:
1. JSON files are never modules (`:61-63`).
2. `isFileProbablyExternalModule` (`:86-93`) returns the first statement that is an `export` modifier, an `import x = require()`, an import declaration, an export assignment or an export declaration. Failing that, it looks for `import.meta`.
3. Declaration files stop here and are not modules (`:69-71`).
4. If `opts.JSX` is set, a JSX element or fragment makes the file a module (`:73-77`, walk at `:129-149`).
5. If `opts.Force` is set, the file itself is the indicator, so it is a module (`:79-81`).
6. Otherwise the file is a script (`:83`).

**Other uses.** `ls/autoimport/fix.go:915` also reads the resolved kind. It checks `!= Force` when deciding auto-import behaviour.

I only read these two functions, not their callers.