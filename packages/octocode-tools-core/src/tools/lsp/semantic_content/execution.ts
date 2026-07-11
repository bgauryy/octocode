import { type CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import { executeBulkOperation } from '../../../utils/response/bulk.js';
import type { ToolExecutionArgs } from '../../../types/execution.js';
import { executeWithToolBoundary } from '../../executionGuard.js';
import {
  acquirePooledClient,
  isLanguageServerAvailable,
} from '@octocodeai/octocode-engine/lsp/manager';
import { resolveWorkspaceRootForFile } from '@octocodeai/octocode-engine/lsp/workspaceRoot';
import {
  LSP_GET_SEMANTICS_TOOL_NAME,
  type LspGetSemanticsQuery,
  type LspSemanticEnvelope,
} from '../shared/semanticTypes.js';
import { attachReadinessWarning } from '../shared/readiness.js';
import { resolveSymbolAnchor } from '../shared/resolveSymbolAnchor.js';
import {
  CONSUMER_SCOPED_TYPES,
  dispatchAnchoredSemantic,
  warmLikelyConsumers,
} from './semanticAnchored.js';
import { failedAnchorEnvelope } from './semanticEnvelopes.js';
import {
  getDocumentSymbols,
  getFileDiagnostics,
  getWorkspaceSymbols,
  throwLspUnavailable,
} from './semanticFileOps.js';
import {
  attachSemanticRawEvidence,
  formatSemanticResult,
  withSemanticNext,
} from './semanticPresentation.js';

export async function executeLspGetSemantics(
  args: ToolExecutionArgs<LspGetSemanticsQuery>
): Promise<CallToolResult> {
  return executeBulkOperation(
    args.queries || [],
    async query => {
      return executeWithToolBoundary({
        toolName: LSP_GET_SEMANTICS_TOOL_NAME,
        query,
        contextMessage: 'lspGetSemantics execution failed',
        execute: async () => {
          const result = await getSemanticContent(query);
          return attachSemanticRawEvidence(
            withSemanticNext(formatSemanticResult(query, result))
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
    const message =
      typeof anchor.error.error === 'string'
        ? anchor.error.error
        : 'Symbol anchor resolution failed';
    return failedAnchorEnvelope(query, message);
  }

  const workspaceRoot =
    query.workspaceRoot ??
    (await resolveWorkspaceRootForFile(anchor.value.uri));
  const serverAvailable = await isLanguageServerAvailable(
    anchor.value.uri,
    workspaceRoot
  );
  if (!serverAvailable) {
    // No server → throw, so the agent pivots to text search. We never return a
    // same-file-only or syntactic approximation dressed up as a semantic answer.
    throwLspUnavailable(anchor.value.uri, query.type);
  }

  const client = await acquirePooledClient(workspaceRoot, anchor.value.uri);
  if (!client) {
    throwLspUnavailable(anchor.value.uri, query.type);
  }

  if (CONSUMER_SCOPED_TYPES.has(query.type)) {
    await warmLikelyConsumers(client, anchor.value, workspaceRoot);
  }

  // Readiness recorded when the pooled client warmed. `undefined` = the wait
  // was skipped for a server that answers immediately (no indexing caveat).
  const readiness = client.getReadiness?.();

  const envelope = await dispatchAnchoredSemantic(query, anchor.value, client);
  return attachReadinessWarning(envelope, readiness);
}
