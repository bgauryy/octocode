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

// 1-based line/character (matching editor display and the other LSP tool
// outputs) — raw LSP 0-based ranges are an off-by-one trap for agents.
type DiagnosticEntry = {
  line: number;
  character: number;
  endLine?: number;
  severity: 'error' | 'warning' | 'information' | 'hint';
  message: string;
  source?: string;
  code?: unknown;
  relatedInformation?: Array<{ uri: string; line: number; message: string }>;
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
  if (anchor.ok === false) {
    const message =
      typeof anchor.error.error === 'string'
        ? anchor.error.error
        : 'File resolution failed';
    const hints = Array.isArray(anchor.error.hints)
      ? (anchor.error.hints as string[])
      : [`File could not be resolved: ${query.uri ?? query.filePath ?? ''}`];
    // diagnostics/summary omitted — zero counts would read as "file is clean".
    return {
      uri: query.uri ?? query.filePath ?? '',
      lsp: { serverAvailable: false, source: 'unavailable' as const },
      warnings: [message],
      hints,
    };
  }

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
    // diagnostics/summary omitted — zero counts would read as "file is clean".
    return {
      uri: anchor.value.uri,
      lsp: { serverAvailable: false, source: 'unavailable' },
      warnings: ['Language server unavailable'],
      hints: diagnosticHints('unavailable', true),
    };
  }

  const result = await client.getDiagnostics(
    anchor.value.uri,
    anchor.value.content
  );
  const allEntries = result.diagnostics.map(toDiagnosticEntry);
  const filtered = allEntries
    .filter(entry => matchesSeverity(entry, query.severity ?? 'all'))
    .filter(entry => !query.source || entry.source === query.source);
  // Diagnostics hidden ONLY by the severity/source filter — without this
  // signal a filtered-out file reads as a false "clean".
  const filteredOutCount = allEntries.length - filtered.length;

  const DEFAULT_DIAGNOSTICS_PER_PAGE = 50;
  const page = Math.max(1, query.page ?? 1);
  const itemsPerPage = Math.max(
    1,
    query.itemsPerPage ?? DEFAULT_DIAGNOSTICS_PER_PAGE
  );
  const totalDiagnostics = filtered.length;
  const totalPages = Math.max(1, Math.ceil(totalDiagnostics / itemsPerPage));
  const startIdx = (page - 1) * itemsPerPage;
  const diagnostics = filtered.slice(startIdx, startIdx + itemsPerPage);
  const pagination =
    totalDiagnostics > itemsPerPage
      ? {
          currentPage: page,
          totalPages,
          itemsPerPage,
          totalDiagnostics,
          hasMore: page < totalPages,
        }
      : undefined;

  return {
    uri: anchor.value.uri,
    lsp: { serverAvailable: true, source: result.source },
    diagnostics,
    ...(pagination && { pagination }),
    summary: summarize(filtered),
    warnings:
      result.source === 'unavailable'
        ? [
            'Diagnostics unavailable; server did not advertise pull diagnostics and no publishDiagnostics buffer was available.',
          ]
        : undefined,
    hints: diagnosticHints(
      result.source,
      filtered.length === 0,
      filtered.filter(d => d.severity === 'error').length,
      filteredOutCount,
      pagination
    ),
  };
}

function toDiagnosticEntry(diagnostic: Diagnostic): DiagnosticEntry {
  const line = diagnostic.range.start.line + 1;
  const endLine = diagnostic.range.end.line + 1;
  const related = (diagnostic.relatedInformation ?? [])
    .filter(info => info?.location?.uri)
    .map(info => ({
      uri: info.location.uri,
      line: (info.location.range?.start?.line ?? 0) + 1,
      message: info.message,
    }));
  return {
    line,
    character: diagnostic.range.start.character + 1,
    ...(endLine !== line && { endLine }),
    severity: severityName(diagnostic.severity),
    message: diagnostic.message,
    source: diagnostic.source,
    code: diagnostic.code,
    ...(related.length > 0 && { relatedInformation: related }),
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

// Threshold semantics (matching the schema text "Minimum severity:
// error < warning < information < hint"): severity="warning" includes
// errors AND warnings — exact-match filtering would zero out a file that
// only has errors, reading as a false "clean".
const SEVERITY_RANK: Record<DiagnosticEntry['severity'], number> = {
  error: 1,
  warning: 2,
  information: 3,
  hint: 4,
};

function matchesSeverity(
  diagnostic: DiagnosticEntry,
  requested: LspDiagnosticsQuery['severity']
): boolean {
  if (!requested || requested === 'all') return true;
  return SEVERITY_RANK[diagnostic.severity] <= SEVERITY_RANK[requested];
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
