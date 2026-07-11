/**
 * Local execution adapter: compile a canonical OQL query into the existing
 * local tool runners (`searchContentRipgrep`, `findFiles`, `viewStructure`,
 * `fetchContent`) and map their typed results into OQL rows.
 *
 * These lower-level runners perform path validation internally, so the
 * security contract (path bounds) survives this adapter. Secret sanitization
 * is applied by the interface layer on final output.
 *
 * This file is a thin orchestrator + barrel: the actual per-target execution
 * lives in sibling modules under ./local/ (kept under the max-lines cap).
 */
import fs from 'node:fs';
import path from 'node:path';
import { diagnostic } from '../diagnostics.js';
import { executeCode } from './local/code.js';
import { executeFiles } from './local/files.js';
import { executeStructure, executeContent } from './local/structureContent.js';
import { firstScopePath } from '../transformers/github/common.js';
import type { OqlQuery, QuerySource } from '../types.js';
import type { AdapterResult } from './local/types.js';

export type { AdapterResult };

/** Resolve the local filesystem root for a query (local or materialized). */
function localRoot(query: OqlQuery): string {
  if (query.from?.kind === 'local') return query.from.path;
  if (query.from?.kind === 'materialized') return query.from.localPath;
  throw new Error('localExecute requires a local or materialized source.');
}

export async function executeLocal(query: OqlQuery): Promise<AdapterResult> {
  // dispatch only routes local/materialized code/content/structure/files here.
  const source = query.from as QuerySource;
  const root = localRoot(query);
  const scopePath = firstScopePath(query.scope);
  const searchPath = scopePath ? path.join(root, scopePath) : root;

  // Path-existence guard: ripgrep/find/structure on a non-existent path return
  // zero rows, which the adapter would otherwise map to a clean `zeroMatches`
  // with evidence:proof / answerReady:true — falsely confirming absence when
  // the path was simply a typo. Surface a blocking `invalidQuery` error instead
  // so an agent corrects the path rather than concluding "not found".
  if (!fs.existsSync(searchPath)) {
    return {
      results: [],
      diagnostics: [
        diagnostic(
          'invalidQuery',
          `Local path does not exist: ${searchPath}. Check the path/spelling (and branch or materialization for remote sources) before treating this as absence.`,
          {
            backend: 'localExecute',
            queryPath: searchPath,
            repair: {
              message:
                'Verify the path exists (orient with target:"structure" on a known-good parent), fix typos, or materialize the remote source first.',
            },
          }
        ),
      ],
      provenance: [],
    };
  }

  switch (query.target) {
    case 'files':
      return executeFiles(query, source, searchPath);
    case 'structure':
      return executeStructure(query, source, searchPath);
    case 'content':
      return executeContent(query, source, searchPath);
    case 'code':
    default:
      return executeCode(query, source, searchPath);
  }
}
