import type { CallToolResult } from '@modelcontextprotocol/server';
import { executeBulkOperation } from '../../../utils/response/bulk/response.js';
import type { ToolExecutionArgs } from '../../../types/execution.js';
import { executeWithToolBoundary } from '../../executionGuard.js';
import { safeParseOrError } from '../../utils.js';
import { acquirePooledClientDetailed } from '@octocodeai/octocode-engine/lsp/manager';
import { resolveWorkspaceRootForFile } from '@octocodeai/octocode-engine/lsp/workspaceRoot';
import { LSP_SEARCH_TOOL_NAME } from '@octocodeai/octocode-core/schema';
import {
  type LspSearchQuery,
  type LspSemanticEnvelope,
  type ResolvedLanguageServerReceipt,
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
import {
  guardSemanticSnapshot,
  describeRustContext,
} from './semanticSnapshot.js';
import { LspSearchQuerySchema } from '@octocodeai/octocode-core/schema';

export async function executeLspSearch(
  args: ToolExecutionArgs<LspSearchQuery>
): Promise<CallToolResult> {
  return executeBulkOperation(
    args.queries || [],
    async query => {
      const parsed = safeParseOrError(LspSearchQuerySchema, query);
      if (parsed.ok === false) return parsed.error;
      const validatedQuery = parsed.data as LspSearchQuery;

      return executeWithToolBoundary({
        toolName: LSP_SEARCH_TOOL_NAME,
        query: validatedQuery,
        contextMessage: 'lspSearch execution failed',
        execute: async () => {
          const result = await getSemanticContent(validatedQuery);
          const contextualResult = validatedQuery.rustContext
            ? { ...result, rustContext: describeRustContext(validatedQuery) }
            : result;
          return attachSemanticRawEvidence(
            formatSemanticResult(
              validatedQuery,
              withSemanticNext(
                validatedQuery,
                classifySemanticResult(
                  guardSemanticSnapshot(validatedQuery, contextualResult)
                )
              )
            )
          );
        },
      });
    },
    {
      toolName: LSP_SEARCH_TOOL_NAME,
      minQueryTimeoutMs: 30_000,
    },
    args
  );
}

async function getSemanticContent(
  query: LspSearchQuery
): Promise<LspSemanticEnvelope | Record<string, unknown>> {
  if (query.operation === 'documentSymbols') {
    return getDocumentSymbols(query);
  }
  if (query.operation === 'workspaceSymbol') {
    return getWorkspaceSymbols(query);
  }
  if (query.operation === 'diagnostic') {
    return getFileDiagnostics(query);
  }

  const anchor = await resolveSymbolAnchor(query, LSP_SEARCH_TOOL_NAME);
  if (anchor.ok === false) {
    if (anchor.error.status === 'error') return anchor.error;
    const message =
      typeof anchor.error.error === 'string'
        ? anchor.error.error
        : 'Symbol anchor resolution failed';
    return failedAnchorEnvelope(
      query,
      message,
      anchor.error.errorType === 'symbol_not_found'
        ? 'symbolNotFound'
        : 'anchorFailed'
    );
  }

  const workspaceRoot =
    query.workspaceRoot ??
    (await resolveWorkspaceRootForFile(anchor.value.absolutePath));
  const clientResult = await acquirePooledClientDetailed(
    workspaceRoot,
    anchor.value.absolutePath,
    query.rustContext
  );
  if (clientResult.ok === false) {
    throwLspUnavailable(anchor.value.uri, query.operation, clientResult);
  }
  const client = clientResult.client;
  const receipt = (
    clientResult as typeof clientResult & {
      receipt: ResolvedLanguageServerReceipt;
    }
  ).receipt;

  const consumerProvider = CONSUMER_SCOPED_PROVIDERS[query.operation];
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
  return attachReadinessWarning(
    {
      ...envelope,
      workspaceRoot,
      lsp: {
        ...envelope.lsp,
        receipt,
      },
    },
    readiness
  );
}
