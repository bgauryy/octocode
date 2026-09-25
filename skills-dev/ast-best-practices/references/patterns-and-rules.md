# ast-grep and octo patterns and rules

Load when writing an ast-grep pattern or YAML rule, or an octo structural pattern. Why: metavariables match by structure, not text; rule evaluation order is fixed; and fragments that don't parse, or unanchored branches, match nothing or scan every node.

## ast-grep patterns and rules

| Strictness | Compares | Use when |
|---|---|---|
| `Cst` | every node, trivia included | exact reproduction |
| `Smart` (default) | all pattern nodes; skips **unnamed candidate** nodes and comments on the candidate side | normal code search |
| `Ast` | named nodes only | ignore punctuation differences |
| `Relaxed` | named nodes, comments ignored | lenient search |
| `Signature` | node kinds only, text ignored | shape-only matching |
| `Template` | text only, kinds ignored | loose text templates |

- **Metavariables.**
  - `$A` matches one **named** node. `$$A` matches one node, **unnamed included**. `$$$ARGS` matches zero or more. `$_` and `$$$` match without capturing.
  - Names must be `[A-Z_][A-Z0-9_]*`, so `$a` is literal text.
  - A repeated `$A` must match **structurally**, not textually: the same kind with recursively equal children, and text equality only at named leaves. So `a+b` matches `a + b`.
- **Evaluation order is fixed**, whatever the YAML key order: atomic (`pattern`, `kind`, `regex`, `nthChild`, `range`), then composite (`all`, `any`, `not`, `matches`), then relational (`inside`, `has`, `precedes`, `follows`). Inside `all:`, list order is kept, and earlier entries bind metavariables for later ones.
- **`stopBy`.** `neighbor` is the default. `end` searches all ancestors or descendants. A **rule** object stops the search, and the node that matches the stop rule is itself still tested.
- **`kind` takes CSS-like selectors**: `a > b`, `a b`, `+`, `~`. It also inherits the grammar kind traps: supertype names match nothing, and a prefix of `ERROR` matches ERROR nodes (see `references/grammar-and-language.md`).
- **Contextual patterns** for fragments that don't parse alone: `pattern: {context: "class A { $F = 1 }", selector: field_definition}`. The match is the **first** preorder node of `selector` kind inside the context.
- **`constraints`** are checked **after** the whole rule matches and apply only to single captures; `$$$` captures are never constrained. `transform` runs after constraints and rejects cycles. An undefined metavariable in fix, constraints or transform is an error.
- `nthChild` counts **named** siblings, 1-based, and is O(k²) on wide parents. `range` uses a 0-based line and a char column.
- **Anchor rules on `kind` or a pattern.** `find_all` prefilters candidates by `potential_kinds()`. `regex`, text and every relational rule give no kinds. `any` gives none if **any** branch gives none, so each top-level `any` branch needs its own anchor. A Template-strictness pattern or an ERROR-rooted pattern also gives none.
- `Pattern::has_error()` checks **only the root** kind. A nested ERROR in the pattern matches **any** candidate kind. Parse the pattern source yourself and reject it if the tree `has_error()`.
- Compile a `RuleConfig` **once per request**, never once per file inside `par_iter`.

## House octo matcher (`structural/octo/*`)

The matcher is built directly on tree-sitter (ast-grep is used only for rewrite). What to know before extending it:
- **The expando swap.** `$` becomes a per-language character that parses as an identifier: `$` for JS/TS/Java, U+10000 for the C family, `Q` for asm, `µ` otherwise (`structural/language.rs`).
- **Fragment repair.** It appends `;` for Java/Rust/C/C++/Go calls, wraps C# in `class __OctoWrap { … }`, and handles C++ initializer-list ambiguity (`pattern.rs`). A new language usually needs its own repair. Test that the pattern tree has no `ERROR`.
- **Matching rules.** All children, punctuation included, are compared by kind, and leaves by text. `MISSING` candidates never bind. `MISSING` nodes are stripped from the pattern side only.
- **Prefilter.** The longest literal anchor goes to ripgrep before parsing, and `CandidatePlan::Kinds` prunes before a full match.

Next: turning matches into edits → `references/editing.md`.
