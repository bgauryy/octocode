import type { CallToolResult } from '@modelcontextprotocol/server';
import { executeBulkOperation } from '../../../utils/response/bulk/response.js';
import type { ToolExecutionArgs } from '../../../types/execution.js';
import { executeWithToolBoundary } from '../../executionGuard.js';
import { safeParseOrError } from '../../utils.js';
import {
  acquirePooledClientDetailed,
  isLanguageServerAvailable,
} from '@octocodeai/octocode-engine/lsp/manager';
import { resolveWorkspaceRootForFile } from '@octocodeai/octocode-engine/lsp/workspaceRoot';
import { LSP_GET_SEMANTICS_TOOL_NAME } from '../../toolNames.js';
import {
  type LspGetSemanticsQuery,
  type LspSemanticEnvelope,
} from '../shared/semanticTypes.js';
import { attachReadinessWarning } from '../shared/readiness.js';
import { resolveSymbolAnchor } from '../shared/resolveSymbolAnchor.js';
import {
  CONSUMER_SCOPED_PROVIDERS,
  dispatchAnchoredSemantic,
  warmLikelyConsumers,
} from './semanticAnchored.js';
import { failedAnchorEnvelope } from './semanticEnvelopes/envelopeHelpers.js';
import { getDocumentSymbols } from './semanticFileOps/documentSymbols.js';
import {
  getFileDiagnostics,
  getWorkspaceSymbols,
} from './semanticFileOps/workspaceSymbolsAndDiagnostics.js';
import { throwLspUnavailable } from './semanticFileOps/anchor.js';
import {
  attachSemanticRawEvidence,
  classifySemanticResult,
  formatSemanticResult,
} from './semanticPresentation.js';
import { withSemanticNext } from './semanticNext.js';
import { guardSemanticSnapshot } from './semanticSnapshot.js';
import { LspGetSemanticsQuerySchema } from './scheme.js';

export async function executeLspGetSemantics(
  args: ToolExecutionArgs<LspGetSemanticsQuery>
): Promise<CallToolResult> {
  return executeBulkOperation(
    args.queries || [],
    async query => {
      const parsed = safeParseOrError(LspGetSemanticsQuerySchema, query);
      if (parsed.ok === false) return parsed.error;
      const validatedQuery = parsed.data as LspGetSemanticsQuery;

      return executeWithToolBoundary({
        toolName: LSP_GET_SEMANTICS_TOOL_NAME,
        query: validatedQuery,
        contextMessage: 'lspGetSemantics execution failed',
        execute: async () => {
          const result = await getSemanticContent(validatedQuery);
          return attachSemanticRawEvidence(
            formatSemanticResult(
              validatedQuery,
              withSemanticNext(
                validatedQuery,
                classifySemanticResult(
                  guardSemanticSnapshot(validatedQuery, result)
                )
              )
            )
          );
        },
      });
    },
    {
      toolName: LSP_GET_SEMANTICS_TOOL_NAME,
      minQueryTimeoutMs: 30_000,
    },
    args
  );
}

async function getSemanticContent(
  query: LspGetSemanticsQuery
): Promise<LspSemanticEnvelope | Record<string, unknown>> {
  if (query.type === 'documentSymbols') {
    return getDocumentSymbols(query);
  }
  if (query.type === 'workspaceSymbol') {
    return getWorkspaceSymbols(query);
  }
  if (query.type === 'diagnostic') {
    return getFileDiagnostics(query);
  }

  const anchor = await resolveSymbolAnchor(query, LSP_GET_SEMANTICS_TOOL_NAME);
  if (anchor.ok === false) {
    if (anchor.error.status === 'error') return anchor.error;
    const message =
      typeof anchor.error.error === 'string'
        ? anchor.error.error
        : 'Symbol anchor resolution failed';
    return failedAnchorEnvelope(query, message);
  }

  const workspaceRoot =
    query.workspaceRoot ??
    (await resolveWorkspaceRootForFile(anchor.value.absolutePath));
  const serverAvailable = await isLanguageServerAvailable(
    anchor.value.absolutePath,
    workspaceRoot
  );
  if (!serverAvailable) {
    // No server → throw, so the agent pivots to text search. We never return a
    // same-file-only or syntactic approximation dressed up as a semantic answer.
    throwLspUnavailable(anchor.value.uri, query.type);
  }

  const clientResult = await acquirePooledClientDetailed(
    workspaceRoot,
    anchor.value.absolutePath
  );
  if (clientResult.ok === false) {
    throwLspUnavailable(anchor.value.uri, query.type, clientResult);
  }
  const client = clientResult.client;

  const consumerProvider = CONSUMER_SCOPED_PROVIDERS[query.type];
  const warmupStats =
    consumerProvider && client.hasCapability(consumerProvider)
      ? await warmLikelyConsumers(client, anchor.value, workspaceRoot)
      : undefined;

  // Readiness recorded when the pooled client warmed. `undefined` = the wait
  // was skipped for a server that answers immediately (no indexing caveat).
  const readiness = client.getReadiness?.();

  const envelope = await dispatchAnchoredSemantic(
    query,
    anchor.value,
    client,
    warmupStats
  );
  return attachReadinessWarning({ ...envelope, workspaceRoot }, readiness);
}
