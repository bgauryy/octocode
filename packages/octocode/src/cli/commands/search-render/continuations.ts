/** Collecting and formatting `next.*` continuation hints for an envelope. */
import { dim } from '../../../utils/colors.js';
import type {
  OqlContinuation,
  OqlResultEnvelope,
} from '@octocodeai/octocode-tools-core/oql';

type RenderableContinuation = {
  rawKey: string;
  key: string;
  label: string;
  continuation: OqlContinuation;
  origin?: string;
  hint?: string;
};

export function renderContinuationLines(
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
