/**
 * Helpers shared between the main astRewrite executor and postcondition
 * evaluation — all delegating to the native Rust engine (no external binary).
 */
import type { AstRewriteQuery } from './types.js';

/**
 * Build the complete ast-grep inline-rule JSON accepted by the Rust engine's
 * `structuralRewriteContent` / `structuralRewriteFiles` bindings.
 *
 * For `ruleKind: "pattern"` the shorthand `pattern + rewrite + langType` is
 * expanded to an equivalent `{rule: {pattern}, fix}` object.
 * For `"rule"` and `"experimental"` the full rule is forwarded as-is.
 */
export function buildRuleConfigJson(query: AstRewriteQuery): string {
  if (query.ruleKind === 'pattern') {
    return JSON.stringify({
      id: 'octocode-inline-rewrite',
      language: query.langType,
      severity: 'warning',
      message: 'Octocode inline structural rewrite',
      rule: { pattern: query.pattern },
      fix: query.rewrite,
    });
  }
  return JSON.stringify({
    id: 'octocode-inline-rewrite',
    language: query.langType,
    severity: 'warning',
    message: 'Octocode inline structural rewrite',
    rule: query.rule,
    fix: query.fix,
    ...(query.constraints ? { constraints: query.constraints } : {}),
    ...(query.utils ? { utils: query.utils } : {}),
    ...(query.transform ? { transform: query.transform } : {}),
    ...(query.ruleKind === 'experimental' && query.rewriters
      ? { rewriters: query.rewriters }
      : {}),
  });
}
