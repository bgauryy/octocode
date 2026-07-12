/** Rendering for a single OQL result row, including the `record` dispatcher. */
import { c, dim } from '../../../utils/colors.js';
import type { OqlResultEnvelope } from '@octocodeai/octocode-tools-core/oql';
import {
  renderResearchRecord,
  renderSemanticsRecord,
} from './record-detail.js';

export function renderRow(row: OqlResultEnvelope['results'][number]): string {
  switch (row.kind) {
    case 'code':
      return `  ${c('green', row.path)}${row.line !== undefined ? `:${row.line}` : ''}${row.snippet ? `  ${dim(clip(row.snippet.replace(/\s+/g, ' ').trim(), 200))}` : ''}`;
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
  return detail ? `${head}  ${dim(clip(detail, 200))}` : head;
}

/** Cap display text, marking the cut so a trimmed line is never mistaken for
 * the full value (full content stays available via --json / exact reads). */
function clip(text: string, max: number): string {
  return text.length > max ? `${text.slice(0, max)}…` : text;
}
