# Grammars, languages and parse options

Load when choosing a grammar or `SourceType`, validating a node-kind name, adding a language, or parsing embedded or injected code. Why: the wrong grammar or source type turns valid code into `ERROR` nodes, and some kind names resolve to ids that no node ever has, so a rule matches nothing and raises no error.

## tree-sitter grammars

- **ABI window.** tree-sitter 0.27 accepts grammar ABI 13–15 (`LANGUAGE_VERSION=15`, `MIN_COMPATIBLE=13`). Outside that window, `set_language` returns `LanguageError::Version`.
  - The pinned grammars mix ABIs. typescript/tsx 0.23.2, java and cpp are **ABI 14**. javascript/python/go 0.25, rust and c are 15.
  - On ABI < 15, `Language::name()`, `metadata()`, `supertypes()` and `subtypes_for_supertype()` return None or empty. Don't key a registry on `name()`.
- **`tsx` and `typescript` are two languages.** Only TS has `type_assertion` (`<T>x`). Only TSX has the `jsx_*` kinds. Parse `.ts` with TSX and `<T>x` becomes `ERROR`. Route by extension, never "TSX for everything".
- **Kind names change with grammar versions.** Validate each name once, at compile time, with `language.id_for_node_kind(name, named)`, and reject a result of `0`. Traps:
  - **Supertypes** such as `expression`, `statement` and `declaration` return a **non-zero** id, but no node ever carries it because supertypes are hidden. ast-grep `kind: expression` compiles and matches nothing. Check `node_kind_is_supertype(id)` and reject. In queries, `(expression)` and `(expression/identifier)` do work.
  - **The ERROR prefix bug.** With `named=true`, any prefix of `"ERROR"` returns 65535, including `""`, `"E"` and `"ERR"`. Reject empty and non-exact names yourself.
  - **Hidden rules** (`_foo`) return 0.
- **Aliases.** `kind_id()` and `id_for_node_kind` both give the canonical public symbol, so compare those two. `grammar_id()`/`grammar_name()` are the raw, unaliased symbol: never compare them against `id_for_node_kind`.
- **Fields.** `field_id_for_name` returns `Option<NonZeroU16>`. Resolve it once, then call `child_by_field_id` in the hot loop.
- **0.27 API changes:**
  - `kind()`, `grammar_name()`, `field_name*()` and `node_kind_for_id()` return `&'tree str`/`&str`, no longer `&'static str`. Store kind ids, not kind strings.
  - `child_count()` returns `u32`.
  - The cursor type is now `TreeCursor<'tree>`.
  - `QueryMatch::captures()` and `Query::deep_clone()` are new. Use `deep_clone()` before `disable_*` on a shared query.

## Embedded and injected code

- `parser.set_included_ranges(&ranges)` makes the parser read only those ranges. They must be sorted and non-overlapping (`IncludedRangesError`). An empty slice means the whole file.
- **The setting persists on a reused parser.** Any pooled or `thread_local` parser that may ever see included ranges must call `reset()` **and** `set_included_ranges(&[])` before each file (Zed's pool does both).
- Parse one layer per language: query the host tree for the injection nodes, then parse each language over its ranges. Difftastic supports only one nested layer and splices it in by `node.id()`. Helix (tree-house) and Zed (`SyntaxMap`) keep full layer trees.
- ast-grep `Root::get_injections` builds a new parser and copies the source for each injected language. Treat it as costly.

## oxc `SourceType` and `ParseOptions`

- **Use `SourceType::from_path(path)`**, then override on purpose.
  - It maps `.mjs/.mts` to Module, `.cjs/.cts` to CommonJS, `.js/.jsx/.ts/.tsx` to `Unambiguous` (ESM only if the file has module syntax), and `.d.ts`/`.d.mts`/`.d.cts` to declaration files.
  - `.js` defaults to no JSX, so React code in `.js` needs `.with_jsx(true)`.
  - `SourceType::default()` is strict ESM with no JSX. Using it for all `.js` breaks sloppy scripts, CommonJS top-level `return`, and JSX.
- `ParseOptions` defaults that affect matchers:
  - `preserve_parens: true`: you get `ParenthesizedExpression`/`TSParenthesizedType` nodes, so unwrap them or turn the option off.
  - `allow_return_outside_function: false`.
  - `enable_ident_hashes: true`. Turning it off breaks semantic analysis.
- Hard limits: sources longer than `u32::MAX - 256` bytes are rejected (`overlong_source`). No oxc crate has a recursion depth guard (see `references/walking-oxc-and-depth.md`).

Next: what the parser hands back and how to hold it → `references/concepts-and-handles.md`; oxc-only APIs → `references/oxc-toolkit.md`.
