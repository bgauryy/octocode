/**
 * LSP proof escalation for the `graph` target: upgrade page-bounded symbol
 * packets from heuristic reachability to LSP-proven reference counts, attach
 * reference edges, and surface honest diagnostics when proof is unavailable
 * or paginated.
 *
 * Graph-view construction lives in graphView.ts; the execute* adapters stay
 * in researchTargets.ts.
 */
import nodePath from 'node:path';
import type { CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import { runDirect, firstQueryData, numberFrom, stringFrom } from './runner.js';
import { diagnostic } from '../diagnostics.js';
import type {
  EvidenceEdge,
  EvidenceSubject,
  ResearchEvidencePacket,
} from '../research/packets.js';
import type { OqlDiagnostic, OqlQuery } from '../types.js';
import {
  packetMatchesGraphFilters,
  packetPage,
  type GraphFilters,
} from './graphView.js';

export function shouldRunLspProof(
  mode: 'plan' | 'analyze' | 'prove',
  p: Record<string, unknown>
): boolean {
  if (mode === 'plan') return false;
  if (p.proof === 'none') return false;
  return p.proof === 'lsp' || mode === 'prove';
}

export function graphProofLimit(
  query: OqlQuery,
  p: Record<string, unknown>
): number {
  if (typeof p.proofLimit === 'number') return Math.min(25, p.proofLimit);
  const pageSize = query.itemsPerPage ?? query.limit ?? 5;
  return Math.max(1, Math.min(5, pageSize));
}

export async function escalateGraphPacketsWithLsp(
  root: string,
  query: OqlQuery,
  packets: ResearchEvidencePacket[],
  filters: GraphFilters,
  limit: number
): Promise<OqlDiagnostic[]> {
  const filteredPackets = packets.filter(packet =>
    packetMatchesGraphFilters(packet, filters)
  );
  const pageWindow = packetPage(query, filteredPackets.length);
  const pagePackets = filteredPackets
    .slice(pageWindow.packetsStart, pageWindow.packetsEnd)
    .filter(packet => packet.subject.kind === 'symbol')
    .slice(0, limit);

  const diagnostics: OqlDiagnostic[] = [];
  for (const packet of pagePackets) {
    const proof = await proveSymbolPacketWithLsp(root, packet);
    packet.proof = { ...(packet.proof ?? {}), lsp: proof };

    if (proof.status === 'unavailable' || proof.status === 'error') {
      diagnostics.push(
        diagnostic(
          proof.status === 'unavailable' ? 'lspUnavailable' : 'partialResult',
          proof.message ?? 'LSP proof escalation did not complete.',
          { backend: 'lspGetSemantics', severity: 'warning' }
        )
      );
      continue;
    }

    if (typeof proof.totalReferences !== 'number') {
      diagnostics.push(
        diagnostic(
          'partialResult',
          'LSP proof escalation returned without a numeric reference count.',
          { backend: 'lspGetSemantics', blocksAnswer: true }
        )
      );
      continue;
    }

    packet.missingProof = packet.missingProof.filter(
      item => item.kind !== 'lsp-unavailable'
    );
    attachLspReferenceEdges(packet, proof);
    if (proof.paginationOpen) {
      packet.missingProof.push({
        kind: 'pagination-open',
        severity: 'high',
        location: packet.subject,
      });
      diagnostics.push(
        diagnostic(
          'partialResult',
          'LSP proof result is paginated; follow the semantic continuation before deletion.',
          { backend: 'lspGetSemantics', blocksAnswer: true }
        )
      );
    }

    if (proof.totalReferences === 0) {
      packet.proofStatus = 'confirmed-by-lsp';
      packet.risk = {
        deleteRisk: packet.verdict === 'reachable' ? 'high' : 'medium',
        reason:
          'LSP references found zero non-declaration references for this symbol. Still verify dynamic/framework retention before deleting.',
      };
    } else if (typeof proof.totalReferences === 'number') {
      packet.proofStatus =
        packet.verdict === 'reachable'
          ? 'confirmed-by-lsp'
          : 'conflicting-evidence';
      packet.risk = {
        deleteRisk: 'high',
        reason:
          'LSP found non-declaration references. Inspect proof.lsp.files and next.fetch before deleting.',
      };
    }
  }
  return diagnostics;
}

function attachLspReferenceEdges(
  packet: ResearchEvidencePacket,
  proof: LspPacketProof
): void {
  if (
    proof.status !== 'ok' ||
    !proof.totalReferences ||
    proof.files.length === 0
  ) {
    return;
  }
  const existing = new Set(packet.retainedBy.map(edge => edge.id));
  for (const [i, file] of proof.files.entries()) {
    const from: EvidenceSubject = {
      id: `file:${file}`,
      kind: 'file',
      uri: file,
    };
    const edge: EvidenceEdge = {
      id: `${packet.subject.id}:lsp-ref:${i}`,
      from,
      to: packet.subject,
      relation: 'references',
      source: 'lsp',
      confidence: 'exact',
      flags: file === packet.subject.uri ? ['same-file'] : ['external'],
    };
    if (!existing.has(edge.id)) {
      packet.retainedBy.push(edge);
      existing.add(edge.id);
    }
  }
}

type LspPacketProof = {
  readonly status: 'ok' | 'unavailable' | 'error';
  readonly totalReferences?: number;
  readonly files: readonly string[];
  readonly paginationOpen: boolean;
  readonly message?: string;
};

async function proveSymbolPacketWithLsp(
  root: string,
  packet: ResearchEvidencePacket
): Promise<LspPacketProof> {
  const symbolName = packet.subject.name;
  const lineHint = packet.subject.range?.start.line;
  if (!symbolName || typeof lineHint !== 'number') {
    return {
      status: 'error',
      files: [],
      paginationOpen: false,
      message: 'Symbol packet has no name or line hint for LSP proof.',
    };
  }

  const uri = nodePath.isAbsolute(packet.subject.uri)
    ? packet.subject.uri
    : nodePath.resolve(root, packet.subject.uri);
  try {
    const result = await runDirect('lspGetSemantics', {
      type: 'references',
      uri,
      symbolName,
      lineHint,
      includeDeclaration: false,
      groupByFile: true,
      itemsPerPage: 50,
    });
    const directError = directToolError(result);
    if (directError) {
      return {
        status:
          directError.code === 'localToolsDisabled' ? 'unavailable' : 'error',
        files: [],
        paginationOpen: false,
        message: directError.message,
      };
    }
    const { data, status } = firstQueryData<Record<string, unknown>>(result);
    if (status === 'error') {
      return {
        status: 'error',
        files: [],
        paginationOpen: false,
        message: stringFrom(data?.error) ?? 'lspGetSemantics returned error.',
      };
    }
    const lsp = data?.lsp as
      | { serverAvailable?: boolean; source?: string }
      | undefined;
    if (lsp?.serverAvailable === false) {
      return {
        status: 'unavailable',
        files: [],
        paginationOpen: false,
        message:
          lsp.source === 'native'
            ? 'Language server unavailable; native fallback cannot prove cross-file references.'
            : 'Language server unavailable; reference proof is incomplete.',
      };
    }

    const payload =
      data?.payload && typeof data.payload === 'object'
        ? (data.payload as Record<string, unknown>)
        : undefined;
    const pagination = data?.pagination as
      | { hasMore?: boolean; totalItems?: number }
      | undefined;
    const totalReferences =
      numberFrom(data?.totalReferences) ??
      numberFrom(payload?.totalReferences) ??
      numberFrom(data?.referenceCount) ??
      numberFrom(payload?.referenceCount) ??
      numberFrom(pagination?.totalItems) ??
      countReferenceLikeItems(payload) ??
      countReferenceLikeItems(data);
    return {
      status: 'ok',
      ...(typeof totalReferences === 'number' ? { totalReferences } : {}),
      files: referenceFiles(data, root),
      paginationOpen: pagination?.hasMore === true,
    };
  } catch (err) {
    return {
      status: 'error',
      files: [],
      paginationOpen: false,
      message: err instanceof Error ? err.message : 'Could not run LSP proof.',
    };
  }
}

function directToolError(
  result: CallToolResult
): { code?: string; message: string } | undefined {
  const sc = result.structuredContent;
  if (!sc || typeof sc !== 'object') return undefined;
  const record = sc as Record<string, unknown>;
  if (record.status !== 'error') return undefined;
  const error =
    record.error && typeof record.error === 'object'
      ? (record.error as Record<string, unknown>)
      : undefined;
  return {
    ...(typeof error?.code === 'string' ? { code: error.code } : {}),
    message:
      (typeof error?.message === 'string' && error.message) ||
      (typeof record.code === 'string' && record.code) ||
      'Direct tool call failed.',
  };
}

function countReferenceLikeItems(
  data: Record<string, unknown> | undefined
): number | undefined {
  if (!data) return undefined;
  for (const key of ['references', 'locations', 'results', 'byFile']) {
    const value = data[key];
    if (!Array.isArray(value)) continue;
    if (key !== 'byFile') return value.length;
    return value.reduce((total, item) => {
      if (!item || typeof item !== 'object') return total + 1;
      const count = (item as Record<string, unknown>).count;
      return total + (typeof count === 'number' ? count : 1);
    }, 0);
  }
  return undefined;
}

function referenceFiles(
  data: Record<string, unknown> | undefined,
  root: string
): readonly string[] {
  const out = new Set<string>();
  collectReferenceFiles(data, out, root);
  return [...out].slice(0, 25);
}

function collectReferenceFiles(
  value: unknown,
  out: Set<string>,
  root: string
): void {
  if (out.size >= 25 || value === null || value === undefined) return;
  if (Array.isArray(value)) {
    for (const item of value) collectReferenceFiles(item, out, root);
    return;
  }
  if (typeof value !== 'object') return;
  const record = value as Record<string, unknown>;
  for (const key of ['uri', 'file', 'path']) {
    const maybeFile = record[key];
    if (typeof maybeFile === 'string' && looksLikePath(maybeFile)) {
      out.add(
        nodePath.isAbsolute(maybeFile)
          ? nodePath.relative(root, maybeFile)
          : maybeFile
      );
    }
  }
  for (const key of [
    'references',
    'locations',
    'byFile',
    'results',
    'files',
    'groups',
    'items',
  ]) {
    collectReferenceFiles(record[key], out, root);
  }
}

function looksLikePath(value: string): boolean {
  return (
    value.includes('/') || value.includes('\\') || /\.[cm]?[jt]sx?$/.test(value)
  );
}
