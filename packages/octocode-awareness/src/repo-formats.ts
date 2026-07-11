/**
 * repo-context.ts - generated repo-readable projections over awareness data.
 *
 * The SQLite store remains canonical. This module only reads it into lean views
 * for agents/humans and writes optional `.octocode/*` snapshots for workspaces
 * that choose to share or keep a local generated context folder.
 */
import { spawnSync } from 'node:child_process';
import { isAbsolute, relative } from 'node:path';
import { AwarenessQueryResult, AwarenessQueryRow } from './repo-model.js';
import { completenessText } from './repo-query.js';

export function renderHtmlSection(name: string, rows: AwarenessQueryRow[]): string {
  return `<section data-section="${escapeHtml(name)}">
  <h2>${escapeHtml(name)} (${rows.length})</h2>
  ${rows.length === 0 ? '<p class="meta">No rows.</p>' : `<table>${renderHtmlTable(rows)}</table>`}
</section>`;
}

export function renderHtmlTable(rows: AwarenessQueryRow[]): string {
  const keys = keysForRows(rows).slice(0, 12);
  const header = `<thead><tr>${keys.map(key => `<th><button type="button" data-key="${escapeHtml(key)}">${escapeHtml(key)}</button></th>`).join('')}</tr></thead>`;
  const body = rows.map(row => {
    const missing = row['missing_file'] === true || Number(row['missing_reference_count'] ?? 0) > 0 || (Array.isArray(row['missing_references']) && row['missing_references'].length > 0);
    return `<tr data-missing="${missing ? 'true' : 'false'}">${keys.map(key => `<td>${escapeHtml(cellToString(row[key]))}</td>`).join('')}</tr>`;
  }).join('\n');
  return `${header}<tbody>${body}</tbody>`;
}

export function keysForRows(rows: AwarenessQueryRow[]): string[] {
  const keys: string[] = [];
  for (const row of rows) {
    for (const key of Object.keys(row)) {
      if (!keys.includes(key)) keys.push(key);
    }
  }
  return keys;
}

export function toCsv(rows: AwarenessQueryRow[]): string {
  if (rows.length === 0) return '';
  const keys = keysForRows(rows);
  return [
    keys.map(csvCell).join(','),
    ...rows.map(row => keys.map(key => csvCell(cellToString(row[key]))).join(',')),
  ].join('\n') + '\n';
}

export function toTable(rows: AwarenessQueryRow[]): string {
  if (rows.length === 0) return 'No rows.\n';
  const keys = keysForRows(rows).slice(0, 10);
  const widths = keys.map(key => Math.min(40, Math.max(key.length, ...rows.map(row => cellToString(row[key]).length))));
  const line = keys.map((key, i) => key.padEnd(widths[i] ?? key.length)).join('  ');
  const sep = widths.map(w => '-'.repeat(w)).join('  ');
  const body = rows.map(row => keys.map((key, i) => truncate(cellToString(row[key]), widths[i] ?? 40).padEnd(widths[i] ?? 40)).join('  '));
  return [line, sep, ...body].join('\n') + '\n';
}

export function toMarkdown(result: AwarenessQueryResult): string {
  const lines = [
    `# Awareness ${result.view}`,
    '',
    `Generated: ${result.generated_at}`,
    `Workspace: ${result.workspace_path ?? 'global'}`,
    completenessText(result),
    '',
  ];
  if (result.sections) {
    for (const [name, section] of Object.entries(result.sections)) {
      lines.push(`## ${name} (${section.count})`, '', markdownRows(section.rows), '');
    }
  } else {
    lines.push(markdownRows(result.rows), '');
  }
  return lines.join('\n');
}

export function markdownRows(rows: AwarenessQueryRow[]): string {
  if (rows.length === 0) return '_No rows._';
  return rows.map(row => {
    const id = row['memory_id'] ?? row['plan_id'] ?? row['task_id'] ?? row['run_id'] ?? row['signal_id'] ?? row['refinement_id'] ?? row['file_path'] ?? row['metric'] ?? 'row';
    const label = row['label'] ? `[${cellToString(row['label'])}:${cellToString(row['importance'])}] ` : '';
    const title = row['task_context'] ?? row['subject'] ?? row['remember'] ?? row['name'] ?? row['title'] ?? row['rationale'] ?? row['metric'] ?? '';
    const text = row['observation'] ?? row['objective'] ?? row['reasoning'] ?? row['count'] ?? '';
    const extras: string[] = [];
    if (row['failure_signature']) extras.push(`failure=${cellToString(row['failure_signature'])}`);
  if (Array.isArray(row['references']) && row['references'].length > 0) extras.push(`refs=${(row['references'] as string[]).join(', ')}`);
  if (Array.isArray(row['missing_references']) && row['missing_references'].length > 0) extras.push(`missing=${(row['missing_references'] as string[]).join(', ')}`);
    const suffix = extras.length > 0 ? ` (${extras.join('; ')})` : '';
    return `- \`${cellToString(id)}\` ${label}${summarize(cellToString(title), 100)} - ${summarize(cellToString(text), 220)}${suffix}`;
  }).join('\n');
}

export function csvCell(value: unknown): string {
  let s = String(value ?? '');
  if (/^\s*[=+\-@]/.test(s)) s = `'${s}`;
  return /[",\n\r]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s;
}

export function cellToString(value: unknown): string {
  if (Array.isArray(value)) return value.join('; ');
  if (value == null) return '';
  if (typeof value === 'object') return JSON.stringify(value);
  return String(value);
}

export function truncate(value: string, width: number): string {
  if (value.length <= width) return value;
  if (width <= 3) return '.'.repeat(width);
  return value.slice(0, width - 3) + '...';
}

export function summarize(value: string, max: number): string {
  const flat = value.replace(/\s+/g, ' ').trim();
  if (flat.length <= max) return flat;
  return flat.slice(0, Math.max(0, max - 3)).trimEnd() + '...';
}

export function lineCount(value: string): number {
  if (!value) return 0;
  return value.split(/\r\n|\r|\n/).length;
}

export function escapeHtml(value: string): string {
  return value
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}

export function gitCheckIgnored(cwd: string, path: string): { ignored: boolean } {
  const candidate = isAbsolute(path) ? relative(cwd, path) : path;
  const result = spawnSync('git', ['check-ignore', '-q', candidate], { cwd, encoding: 'utf8' });
  return { ignored: result.status === 0 };
}
