import { contextUtils } from '../../utils/contextUtils.js';
import { buildRuleConfigJson } from './nativeRewrite.js';
import { rewriteError } from './result.js';
import type {
  AstRewriteError,
  AstRewriteQuery,
  AstRewriteRuntimeDeps,
  PreparedFile,
} from './types.js';

export async function evaluatePostconditions(
  query: AstRewriteQuery,
  _executable: string,
  files: PreparedFile[],
  _deps: AstRewriteRuntimeDeps
): Promise<
  | { ok: true; remainingMatches: number }
  | { ok: false; result: AstRewriteError }
> {
  if (!query.postconditions || query.postconditions.length === 0)
    return { ok: true, remainingMatches: 0 };

  const ruleConfigJson = buildRuleConfigJson(query);
  let remainingMatches = 0;
  try {
    for (const file of files) {
      const content = file.after.toString('utf8');
      const resultJson = contextUtils.structuralRewriteContent(
        content,
        ruleConfigJson
      );
      const matches: unknown[] = JSON.parse(resultJson);
      remainingMatches += matches.length;
    }
  } catch (execError) {
    return {
      ok: false,
      result: rewriteError(
        'ast.rewrite.postcondition_failed',
        execError instanceof Error
          ? execError.message
          : 'Native postcondition evaluation failed.'
      ),
    };
  }

  for (const postcondition of query.postconditions) {
    if (remainingMatches !== postcondition.equals) {
      return {
        ok: false,
        result: rewriteError(
          'ast.rewrite.postcondition_failed',
          'A staged rewrite postcondition failed; no files were changed.',
          {
            details: {
              kind: postcondition.kind,
              expected: postcondition.equals,
              observed: remainingMatches,
            },
          }
        ),
      };
    }
  }

  return { ok: true, remainingMatches };
}
