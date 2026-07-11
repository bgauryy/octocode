/** Top-level OQL result rendering: batch/envelope dispatch, plan, diagnostics,
 * evidence footer, and rows-only / raw-content lanes. */
import { bold, c, dim } from '../../../utils/colors.js';
import {
  type OqlResultEnvelope,
  type OqlRunResult,
  isBatchEnvelope,
} from '@octocodeai/octocode-tools-core/oql';
import { renderContinuationLines } from './continuations.js';
import { renderRow } from './row.js';

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

/**
 * Rows-only rendering for token-frugal agent loops (--quiet): result rows
 * through the standard row renderer, with NO plan block, pagination note,
 * diagnostics, evidence footer, or continuation hints. Batch envelopes render
 * the merged view when present, otherwise each child's rows.
 */
export function renderRows(result: OqlRunResult): string {
  if (isBatchEnvelope(result)) {
    const envelopes = result.merged
      ? [result.merged]
      : result.children.map(child => child.envelope);
    return envelopes
      .flatMap(env => env.results.map(row => renderRow(row)))
      .join('\n');
  }
  return result.results.map(row => renderRow(row)).join('\n');
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
