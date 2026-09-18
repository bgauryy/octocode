import { createHash } from 'node:crypto';
import type { AstRewriteQuery, PreparedFile } from './types.js';

export function createRewriteSnapshot(
  query: AstRewriteQuery,
  executable: {
    path: string;
    version: string;
    sha256: string;
    capabilityDigest: string;
  },
  realRoot: string,
  files: PreparedFile[],
  matchIds: string[],
  maxFiles: number,
  pageSize: number
): string {
  return createHash('sha256')
    .update(
      JSON.stringify({
        contract: 1,
        executable,
        root: realRoot,
        langType: query.langType,
        ruleSpec:
          query.ruleKind === 'rule' || query.ruleKind === 'experimental'
            ? {
                ruleKind: query.ruleKind,
                rule: query.rule,
                constraints: query.constraints,
                utils: query.utils,
                transform: query.transform,
                fix: query.fix,
                ...(query.ruleKind === 'experimental'
                  ? { rewriters: query.rewriters }
                  : {}),
              }
            : {
                ruleKind: 'pattern',
                pattern: query.pattern,
                rewrite: query.rewrite,
              },
        include: query.include ?? [],
        exclude: query.exclude ?? [],
        maxFiles,
        maxMatches: query.maxMatches,
        pageSize,
        files: files.map(file => [
          file.absolutePath,
          file.beforeHash,
          file.afterHash,
        ]),
        matchIds,
      })
    )
    .digest('hex');
}
