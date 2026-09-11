import { createHash } from 'node:crypto';
import type { PreparedFile } from './prepare.js';
import type { AstRewriteQuery } from './types.js';

export function createRewriteSnapshot(
  query: AstRewriteQuery,
  executable: { path: string; version: string },
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
        pattern: query.pattern,
        rewrite: query.rewrite,
        include: query.include ?? [],
        exclude: query.exclude ?? [],
        maxFiles,
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
