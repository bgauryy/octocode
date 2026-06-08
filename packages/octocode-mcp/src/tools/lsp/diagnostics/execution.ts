import { type CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import {
  DiagnosticSeverity,
  type Diagnostic,
} from 'vscode-languageserver-protocol';
import { executeBulkOperation } from '../../../utils/response/bulk.js';
import {
  attachRawResponseChars,
  countSerializedChars,
} from '../../../utils/response/charSavings.js';
import type { ToolExecutionArgs } from '../../../types/execution.js';
import { executeWithToolBoundary } from '../../executionGuard.js';
import {
  acquirePooledClient,
  isLanguageServerAvailable,
} from '../../../lsp/manager.js';
import { resolveWorkspaceRootForFile } from '../../../lsp/workspaceRoot.js';
import { resolveFileAnchor } from '../shared/resolveSymbolAnchor.js';
import {
  LSP_GET_DIAGNOSTICS_TOOL_NAME,
  type LspDiagnosticsQuery,
} from '../shared/semanticTypes.js';
import { diagnosticHints } from './hints.js';

type DiagnosticEntry = {
  range: Diagnostic['range'];
  severity: 'error' | 'warning' | 'information' | 'hint';
  message: string;
  source?: string;
  code?: unknown;
  relatedInformation?: Diagnostic['relatedInformation'];
};

export async function executeLspGetDiagnostics(
  args: ToolExecutionArgs<LspDiagnosticsQuery>
): Promise<CallToolResult> {
  return executeBulkOperation(
    args.queries || [],
    async query =>
      executeWithToolBoundary({
        toolName: LSP_GET_DIAGNOSTICS_TOOL_NAME,
        query,
        contextMessage: 'lspGetDiagnostics execution failed',
        execute: async () =>
          attachDiagnosticsRawEvidence(await getDiagnostics(query)),
      }),
    {
      toolName: LSP_GET_DIAGNOSTICS_TOOL_NAME,
      peerHints: true,
      peerEvidence: true,
      minQueryTimeoutMs: 30_000,
    },
    args
  );
}

function attachDiagnosticsRawEvidence<T extends object>(result: T): T {
  return attachRawResponseChars(result, countSerializedChars(result));
}

async function getDiagnostics(query: LspDiagnosticsQuery) {
  const anchor = await resolveFileAnchor(query, LSP_GET_DIAGNOSTICS_TOOL_NAME);
  if (anchor.ok === false) return anchor.error;

  const workspaceRoot =
    query.workspaceRoot ??
    (await resolveWorkspaceRootForFile(anchor.value.uri));
  const serverAvailable = await isLanguageServerAvailable(
    anchor.value.uri,
    workspaceRoot
  );
  const client = serverAvailable
    ? await acquirePooledClient(workspaceRoot, anchor.value.uri)
    : null;

  if (!client) {
    return {
      uri: anchor.value.uri,
      lsp: { serverAvailable: false, source: 'unavailable' },
      diagnostics: [],
      summary: { errors: 0, warnings: 0, information: 0, hints: 0 },
      warnings: ['Language server unavailable'],
      hints: diagnosticHints('unavailable', true),
    };
  }

  const result = await client.getDiagnostics(
    anchor.value.uri,
    anchor.value.content
  );
  const diagnostics = result.diagnostics
    .map(toDiagnosticEntry)
    .filter(entry => matchesSeverity(entry, query.severity ?? 'all'))
    .filter(entry => !query.source || entry.source === query.source);

  return {
    uri: anchor.value.uri,
    lsp: { serverAvailable: true, source: result.source },
    diagnostics,
    summary: summarize(diagnostics),
    warnings:
      result.source === 'unavailable'
        ? [
            'Diagnostics unavailable; server did not advertise pull diagnostics and no publishDiagnostics buffer was available.',
          ]
        : undefined,
    hints: diagnosticHints(result.source, diagnostics.length === 0),
  };
}

function toDiagnosticEntry(diagnostic: Diagnostic): DiagnosticEntry {
  return {
    range: diagnostic.range,
    severity: severityName(diagnostic.severity),
    message: diagnostic.message,
    source: diagnostic.source,
    code: diagnostic.code,
    relatedInformation: diagnostic.relatedInformation,
  };
}

function severityName(
  severity: DiagnosticSeverity | undefined
): DiagnosticEntry['severity'] {
  switch (severity) {
    case DiagnosticSeverity.Warning:
      return 'warning';
    case DiagnosticSeverity.Information:
      return 'information';
    case DiagnosticSeverity.Hint:
      return 'hint';
    case DiagnosticSeverity.Error:
    default:
      return 'error';
  }
}

function matchesSeverity(
  diagnostic: DiagnosticEntry,
  requested: LspDiagnosticsQuery['severity']
): boolean {
  return !requested || requested === 'all' || diagnostic.severity === requested;
}

function summarize(diagnostics: DiagnosticEntry[]) {
  return diagnostics.reduce(
    (summary, diagnostic) => {
      if (diagnostic.severity === 'error') summary.errors += 1;
      else if (diagnostic.severity === 'warning') summary.warnings += 1;
      else if (diagnostic.severity === 'information') summary.information += 1;
      else summary.hints += 1;
      return summary;
    },
    { errors: 0, warnings: 0, information: 0, hints: 0 }
  );
}
