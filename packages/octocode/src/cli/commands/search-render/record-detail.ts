/**
 * Per-recordType detail renderers for OQL `record` rows (semantics,
 * research/graph). Dispatched from row.ts's renderRecord().
 */
import {
  countPart,
  previewList,
  recordArray,
  recordValue,
  renderCallSummary,
  renderDiagnosticSummary,
  renderKindCounts,
  renderLocationSummary,
  renderSymbolAnchor,
  renderSymbolSummary,
  stringField,
} from './value-helpers.js';

export function renderSemanticsRecord(d: Record<string, unknown>): string {
  const get = (k: string): string | undefined =>
    d[k] === undefined || d[k] === null ? undefined : String(d[k]);
  const payload = recordValue(d.payload);
  const summary = recordValue(d.summary);
  const resolved = recordValue(d.resolvedSymbol);
  const type = get('type') ?? stringField(payload, 'kind');
  const resolvedName = renderSymbolAnchor(resolved);

  if (
    type === 'documentSymbols' ||
    stringField(payload, 'kind') === 'documentSymbols'
  ) {
    const symbols = recordArray(payload?.symbols);
    const total =
      stringField(summary, 'totalSymbols') ??
      stringField(payload, 'totalSymbols') ??
      String(symbols.length);
    return [
      type,
      `symbols=${stringField(summary, 'returnedSymbols') ?? String(symbols.length)}/${total}`,
      renderKindCounts(recordValue(summary?.kinds)),
      previewList(symbols.map(renderSymbolSummary), 5),
    ]
      .filter(Boolean)
      .join('  ');
  }

  const locations = recordArray(payload?.locations);
  if (locations.length > 0 || type === 'references') {
    return [
      type,
      resolvedName,
      countPart(
        'refs',
        stringField(payload, 'totalReferences') ?? String(locations.length)
      ),
      countPart('files', stringField(payload, 'totalFiles')),
      previewList(locations.map(renderLocationSummary), 3),
    ]
      .filter(Boolean)
      .join('  ');
  }

  const calls = recordArray(payload?.calls);
  if (calls.length > 0 || type === 'callers' || type === 'callees') {
    return [
      type,
      renderSymbolAnchor(recordValue(payload?.root)) ?? resolvedName,
      countPart('incoming', stringField(payload, 'incomingCalls')),
      countPart('outgoing', stringField(payload, 'outgoingCalls')),
      previewList(calls.map(renderCallSummary), 3),
    ]
      .filter(Boolean)
      .join('  ');
  }

  const diagnostics = recordArray(payload?.diagnostics);
  if (diagnostics.length > 0 || type === 'diagnostic') {
    return [
      type,
      countPart('diagnostics', String(diagnostics.length)),
      previewList(diagnostics.map(renderDiagnosticSummary), 3),
    ]
      .filter(Boolean)
      .join('  ');
  }

  return [type, resolvedName, get('uri')].filter(Boolean).join('  ');
}

export function renderResearchRecord(d: Record<string, unknown>): string {
  const summary =
    d.summary && typeof d.summary === 'object' && !Array.isArray(d.summary)
      ? (d.summary as Record<string, unknown>)
      : {};
  const n = (key: string): string | undefined =>
    typeof summary[key] === 'number' ? String(summary[key]) : undefined;
  const parts = [
    typeof d.intent === 'string' ? `intent=${d.intent}` : undefined,
    renderPacketSummary(d),
    n('sourceFiles') && `files=${n('sourceFiles')}`,
    n('unusedFiles') && `unusedFiles=${n('unusedFiles')}`,
    n('exportedSymbols') && `symbols=${n('exportedSymbols')}`,
    n('candidateUnusedExports') &&
      `candidateExports=${n('candidateUnusedExports')}`,
    n('transitiveDeadExports') &&
      `transitiveDead=${n('transitiveDeadExports')}`,
    n('unlistedDependencies') && `unlistedDeps=${n('unlistedDependencies')}`,
    n('unusedDependencies') && `unusedDeps=${n('unusedDependencies')}`,
    n('duplicateDependencies') && `duplicateDeps=${n('duplicateDependencies')}`,
  ].filter(Boolean);
  return parts.join('  ');
}

function renderPacketSummary(d: Record<string, unknown>): string | undefined {
  const packets = recordArray(d.packets);
  if (packets.length === 0) return undefined;
  return `packets=${previewList(packets.map(renderPacketId), 3)}`;
}

function renderPacketId(packet: Record<string, unknown>): string {
  const subject = recordValue(packet.subject);
  const id = stringField(subject, 'id') ?? 'packet';
  const verdict = stringField(packet, 'verdict');
  const proofStatus = stringField(packet, 'proofStatus');
  const suffix = [verdict, proofStatus].filter(Boolean).join('/');
  return suffix ? `${id}[${suffix}]` : id;
}
