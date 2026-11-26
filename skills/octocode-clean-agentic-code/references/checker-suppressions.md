Load when a candidate is a checker suppression: a type escape, lint disable, dead-code allowance, or complexity or safety lint.

# Checker suppressions

Skip generated, vendored, fixture, third-party suppressions. List them with `node <skill-folder>/scripts/scan-signals.mjs [paths] --category suppression` ([scan-signals](../scripts/scan-signals.mjs)). Keep when → else:

| Signal | Check / action |
|---|---|
| `type-escape` | line says why the type is inexpressible; else real type if typecheck passes, report runtime fixes. |
| Non-null `!` or `as T` on a nullable (not scanned) | adjacent invariant proves presence; else report. |
| `lint-disable` | one item with a reason (file- or crate-wide: reason per lint); else remove if the linter passes. |
| `suppressed-dead-code` | written structural cause (helper shared by test binaries); else delete the item or cause; "parity with the old X" is residue. |
| `complexity-lint` | inventory under [Structure](code-smells.md#structure), never batch-fix; raising a threshold only moves it. |
| `safety-lint` in production | comment says why it cannot fail; else report. |

Sweep:

1. Self-reporting: `#[allow]` → `#[expect]` (`unfulfilled_lint_expectations`); `@ts-ignore` → `@ts-expect-error`; ESLint `--report-unused-disable-directives`; mypy `warn_unused_ignores`; ruff `RUF100`; golangci-lint `nolintlint`.
2. Run the checker for affected supported features, targets and test/production configurations.
3. Delete what none needs; else fix the local cause (feature-gated literal, map for a struct with an unread field, written-out macro arm, dropped import-path hack).
4. Blanket test allowances → tool config (`allow-expect-in-tests`, `allow-panic-in-tests`, ESLint test-glob override).
5. Revert self-reporting forms some configuration leaves unfulfilled; each survivor carries `reason = "..."` or a comment.
