/**
 * GitHub provider adapter: compile a canonical OQL query into the existing
 * GitHub tool runners and map their results. Provider-only lanes (text search,
 * content read, tree). Local-only predicates over GitHub are handled by the
 * materialization adapter, not here.
 *
 * This file is a thin barrel/orchestrator — the per-lane implementations live
 * under ./github/ (execute.ts for the code/files/content/structure lanes,
 * provider-diagnostics.ts for status/error classification, shared.ts for
 * common helpers and types).
 */
import type { AdapterResult } from './local.js';
import type { OqlQuery } from '../types.js';
import {
  githubCode,
  githubContent,
  githubFiles,
  githubStructure,
} from './github/execute.js';

export async function executeGithub(query: OqlQuery): Promise<AdapterResult> {
  switch (query.target) {
    case 'content':
      return githubContent(query);
    case 'structure':
      return githubStructure(query);
    case 'files':
      return githubFiles(query);
    case 'code':
    default:
      return githubCode(query);
  }
}
