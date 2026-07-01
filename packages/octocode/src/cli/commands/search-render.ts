import { c, bold, dim } from '../../utils/colors.js';
import {
  type OqlContinuation,
  type OqlResultEnvelope,
  type OqlRunResult,
  isBatchEnvelope,
} from '@octocodeai/octocode-tools-core/oql';

export function render(result: OqlRunResult, compact: boolean): string {
  if (isBatchEnvelope(result)) {
    const parts = result.children.map(
      child =>
        `${bold(c('cyan', `# query ${child.queryIndex} (${child.queryId})`))}\n` +
        renderEnvelope(child.envelope, compact)
    );
    if (result.merged) {
      parts.push(
        `${bold(c('cyan', '# merged'))}\n` +
          renderEnvelope(result.merged, compact)
      );
    }
    for (const d of result.diagnostics) {
      parts.push(dim(`! ${d.code}: ${d.message}`));
    }
    return parts.join('\n\n');
  }
  return renderEnvelope(result, compact);
}

export function renderRawContent(result: OqlRunResult): string | undefined {
  if (isBatchEnvelope(result)) return undefined;
  const contentRows = result.results.filter(row => row.kind === 'content');
  if (
    contentRows.length === 0 ||
    contentRows.length !== result.results.length
  ) {
    return undefined;
  }
  return contentRows.map(row => row.content).join('\n');
}

function renderEnvelope(env: OqlResultEnvelope, compact: boolean): string {
  const lines: string[] = [];

  if (env.plan) {
    lines.push(bold(c('magenta', 'PLAN')));
    for (const node of env.plan.nodes) {
      lines.push(
        `  ${node.path}  ${routeColor(node.route)}${node.backend ? dim(` -> ${node.backend}`) : ''}`
      );
      if (!compact) lines.push(dim(`    ${node.reason}`));
    }
    if (env.plan.materialization) {
      lines.push(
        dim(
          `  materialize: ${env.plan.materialization.mode} (${env.plan.materialization.reason})`
        )
      );
    }
    lines.push('');
  }

  for (const row of env.results) {
    lines.push(renderRow(row));
  }

  if (env.results.length === 0 && !env.plan) {
    lines.push(dim('  (no results)'));
  }

  if (env.pagination?.hasMore) {
    lines.push(dim('  … more results available (follow next.page)'));
  }

  // If a structural zero-match guidance is present, render it prominently and
  // skip the paired generic "zeroMatches: Query ran and matched nothing." line
  // which adds no information alongside it.
  const diagnosticMessageText = (message: unknown): string =>
    typeof message === 'string'
      ? message
      : message instanceof Error
        ? message.message
        : (JSON.stringify(message) ?? String(message));
  const hasStructuralGuidance = env.diagnostics.some(d =>
    diagnosticMessageText(d.message).startsWith('0 structural')
  );
  for (const d of env.diagnostics) {
    if (hasStructuralGuidance && d.code === 'zeroMatches') continue;
    const message = diagnosticMessageText(d.message);
    if (message.startsWith('0 structural')) {
      // Surface the body-shape hint as a standalone actionable block.
      lines.push(`  ${c('yellow', '⚡ structural pattern tip:')}`);
      for (const part of message
        .replace(/^0 structural matches\.\s*/, '')
        .split(/\s{2,}|\n/)) {
        if (part.trim()) lines.push(`    ${dim(part.trim())}`);
      }
      continue;
    }
    const sev =
      d.severity === 'error'
        ? c('red', '✗')
        : d.severity === 'warning'
          ? c('yellow', '!')
          : dim('·');
    lines.push(`  ${sev} ${dim(d.code)}: ${message}`);
  }

  const ev = env.evidence;
  // answerReady=false means more proof work remains (follow next.* continuations),
  // not that the query failed. Make that distinction visible inline.
  const readyHint =
    !ev.answerReady && ev.kind !== 'unsupported'
      ? '  · follow next.* continuations for more complete proof'
      : '';
  lines.push(
    dim(
      `  evidence: ${ev.kind}  answerReady=${ev.answerReady}  complete=${ev.complete}${readyHint}`
    )
  );

  const continuationLines = renderContinuationLines(env, compact);
  if (continuationLines.length > 0) {
    lines.push('', ...continuationLines);
  }

  return lines.join('\n');
}

type RenderableContinuation = {
  rawKey: string;
  key: string;
  label: string;
  continuation: OqlContinuation;
  origin?: string;
  hint?: string;
};

function renderContinuationLines(
  env: OqlResultEnvelope,
  compact: boolean
): string[] {
  const entries = collectRenderableContinuations(env);
  if (entries.length === 0) return [];

  const lines: string[] = [];
  for (const entry of entries) {
    const origin = entry.origin ? ` (${entry.origin})` : '';
    lines.push(
      dim(`  next.${entry.key}`) + `  ${dim(`${entry.label}${origin}`)}`
    );
    if (!compact && entry.hint) {
      lines.push(dim(`    ${entry.hint}`));
    }
    const command = continuationCommand(entry.continuation);
    if (command) {
      lines.push(dim(`    ${command}`));
    }
  }
  return lines;
}

function collectRenderableContinuations(
  env: OqlResultEnvelope
): RenderableContinuation[] {
  const entries: RenderableContinuation[] = [];

  for (const [rawKey, continuation] of Object.entries(env.next ?? {})) {
    const key = normalizeNextKey(rawKey);
    entries.push({
      rawKey,
      key,
      continuation,
      label: continuationLabel(key),
      hint: continuation.why ?? env.nextHints?.[rawKey]?.why,
    });
  }

  for (const row of env.results) {
    if (row.kind !== 'record') continue;
    if (row.recordType !== 'research' && row.recordType !== 'graph') continue;
    const next = row.next?.['next.graph'];
    if (!next) continue;
    entries.push({
      rawKey: 'next.graph',
      key: 'graph',
      continuation: next,
      label: continuationLabel('graph'),
      origin: row.id ?? row.recordType,
      hint: env.nextHints?.['next.graph']?.why,
    });
  }

  return entries;
}

function normalizeNextKey(rawKey: string): string {
  return rawKey.startsWith('next.') ? rawKey.slice('next.'.length) : rawKey;
}

function continuationLabel(key: string): string {
  switch (key) {
    case 'graph':
      return 'upgrade to LSP proof';
    case 'page':
      return 'next page';
    case 'materialize':
      return 'materialize for local proof';
    case 'charRange':
      return 'next char window';
    default:
      return key;
  }
}

function continuationCommand(
  continuation: OqlContinuation
): string | undefined {
  if (!continuation.query) return undefined;
  return `search --query ${shellQuote(JSON.stringify(continuation.query))}`;
}

function shellQuote(value: string): string {
  return `'${value.replace(/'/g, `'\\''`)}'`;
}

function renderRow(row: OqlResultEnvelope['results'][number]): string {
  switch (row.kind) {
    case 'code':
      return `  ${c('green', row.path)}${row.line !== undefined ? `:${row.line}` : ''}${row.snippet ? `  ${dim(row.snippet.replace(/\s+/g, ' ').trim().slice(0, 200))}` : ''}`;
    case 'file':
      return `  ${c('green', row.path)}${row.entryType === 'directory' ? '/' : ''}`;
    case 'tree':
      return `  ${row.entryType === 'directory' ? c('blue', row.path) + '/' : c('green', row.path)}`;
    case 'content':
      return `  ${c('green', row.path)} [${row.contentView}]\n${row.content}`;
    case 'record':
      return renderRecord(row);
  }
}

/** Render a record row meaningfully per recordType (id + key fields). */
function renderRecord(row: {
  recordType: string;
  id?: string;
  data: Record<string, unknown>;
}): string {
  const d = row.data;
  const get = (k: string): string | undefined =>
    d[k] === undefined || d[k] === null ? undefined : String(d[k]);
  const head = `  ${c('cyan', row.recordType)} ${c('green', row.id ?? '(no id)')}`;
  let detail = '';
  switch (row.recordType) {
    case 'repository':
      detail = [
        get('stars') && `★${get('stars')}`,
        get('language'),
        get('description'),
      ]
        .filter(Boolean)
        .join('  ');
      break;
    case 'package':
      detail = [get('description'), get('repository')]
        .filter(Boolean)
        .join('  ');
      break;
    case 'pullRequest':
      detail = [get('state'), get('title'), get('author')]
        .filter(Boolean)
        .join('  ');
      break;
    case 'commit': {
      const authorRaw = d.author;
      const authorName =
        authorRaw === undefined || authorRaw === null
          ? undefined
          : typeof authorRaw === 'string'
            ? authorRaw
            : (authorRaw as Record<string, unknown>).name != null
              ? String((authorRaw as Record<string, unknown>).name)
              : undefined;
      detail = [get('title') ?? get('messageHeadline'), authorName]
        .filter(Boolean)
        .join('  ');
      break;
    }
    case 'artifact':
      detail = renderArtifactRecord(d);
      break;
    case 'diff':
      detail = [
        get('path') ?? get('filename'),
        get('additions') && `+${get('additions')}`,
        get('deletions') && `-${get('deletions')}`,
      ]
        .filter(Boolean)
        .join('  ');
      break;
    case 'semantics':
      detail = renderSemanticsRecord(d);
      break;
    case 'research':
    case 'graph':
      detail = renderResearchRecord(d);
      break;
  }
  // Concise lanes flatten rows to { value: "…" }; show it rather than a bare
  // record head.
  if (!detail) detail = get('value') ?? '';
  return detail ? `${head}  ${dim(detail.slice(0, 200))}` : head;
}

function renderArtifactRecord(d: Record<string, unknown>): string {
  const get = (k: string): string | undefined =>
    d[k] === undefined || d[k] === null ? undefined : String(d[k]);
  const mode = get('mode');
  const base = [mode, get('format'), get('arch')].filter(Boolean);
  if (mode === 'list') {
    const entries = stringArray(d.entries);
    return [
      ...base,
      get('backend'),
      countPart('entries', get('totalEntries') ?? String(entries.length)),
      previewList(entries, 5),
    ]
      .filter(Boolean)
      .join('  ');
  }
  if (mode === 'inspect') {
    const libraries = stringArray(d.libraries);
    return [
      ...base,
      get('bits') && `${get('bits')}-bit`,
      get('description'),
      countPart('symbols', get('symbolCount')),
      countPart('imports', get('importCount')),
      countPart('exports', get('exportCount')),
      libraries.length ? `libs=${previewList(libraries, 2)}` : undefined,
    ]
      .filter(Boolean)
      .join('  ');
  }
  return [
    ...base,
    get('description'),
    get('localPath') && `localPath=${get('localPath')}`,
  ]
    .filter(Boolean)
    .join('  ');
}

function renderSemanticsRecord(d: Record<string, unknown>): string {
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

function renderResearchRecord(d: Record<string, unknown>): string {
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

function recordValue(value: unknown): Record<string, unknown> | undefined {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : undefined;
}

function recordArray(value: unknown): Record<string, unknown>[] {
  return Array.isArray(value) ? value.filter(recordValue) : [];
}

function stringArray(value: unknown): string[] {
  return Array.isArray(value)
    ? value
        .map(item => (item === undefined || item === null ? '' : String(item)))
        .filter(Boolean)
    : [];
}

function stringField(
  record: Record<string, unknown> | undefined,
  key: string
): string | undefined {
  const value = record?.[key];
  return value === undefined || value === null ? undefined : String(value);
}

function countPart(
  label: string,
  value: string | undefined
): string | undefined {
  return value === undefined ? undefined : `${label}=${value}`;
}

function previewList(items: string[], max: number): string | undefined {
  const cleaned = items.map(item => item.trim()).filter(Boolean);
  if (cleaned.length === 0) return undefined;
  const suffix = cleaned.length > max ? `, +${cleaned.length - max} more` : '';
  return `${cleaned.slice(0, max).join(', ')}${suffix}`;
}

function renderSymbolAnchor(
  symbol: Record<string, unknown> | undefined
): string | undefined {
  const name = stringField(symbol, 'name');
  if (!name) return undefined;
  const line =
    stringField(symbol, 'line') ??
    stringField(symbol, 'foundAtLine') ??
    stringField(symbol, 'selectionLine');
  return line ? `${name}:${line}` : name;
}

function renderSymbolSummary(symbol: Record<string, unknown>): string {
  const anchor = renderSymbolAnchor(symbol);
  const kind = stringField(symbol, 'kind');
  return [anchor, kind].filter(Boolean).join(' ');
}

function renderLocationSummary(location: Record<string, unknown>): string {
  const range = recordValue(location.displayRange);
  const line = stringField(range, 'startLine');
  const uri = stringField(location, 'uri');
  const content = stringField(location, 'content');
  return [
    uri && line ? `${uri}:${line}` : uri,
    content ? content.trim().slice(0, 80) : undefined,
  ]
    .filter(Boolean)
    .join(' ');
}

function renderCallSummary(call: Record<string, unknown>): string {
  const item = recordValue(call.item);
  const anchor = renderSymbolAnchor(item);
  const ranges = recordArray(call.ranges);
  return [anchor, ranges.length ? `ranges=${ranges.length}` : undefined]
    .filter(Boolean)
    .join(' ');
}

function renderDiagnosticSummary(diagnostic: Record<string, unknown>): string {
  return [
    stringField(diagnostic, 'severity'),
    stringField(diagnostic, 'message')?.slice(0, 80),
  ]
    .filter(Boolean)
    .join(': ');
}

function renderKindCounts(
  kinds: Record<string, unknown> | undefined
): string | undefined {
  if (!kinds) return undefined;
  const parts = Object.entries(kinds).map(
    ([kind, count]) => `${kind}=${count}`
  );
  return parts.length > 0 ? parts.join(' ') : undefined;
}

function routeColor(route: string): string {
  switch (route) {
    case 'PUSHDOWN':
      return c('green', route);
    case 'ROUTE':
      return c('cyan', route);
    case 'RESIDUAL':
      return c('yellow', route);
    default:
      return c('red', route);
  }
}
