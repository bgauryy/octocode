/**
 * Minimal TSV serializer for tool responses.
 *
 * Tab-delimited because tabs almost never appear in source code / paths,
 * while commas appear constantly. RFC-4180-style quote wrapping is replaced
 * with literal-escape encoding (`\t`, `\n`, `\r`, `\\`) so every row stays
 * on a single line — easier for downstream LLM consumers to scan than
 * quote-spanning multi-line records.
 *
 * Zero dependencies on purpose; reviewable in 30 seconds.
 */

const ESCAPE = /[\t\n\r\\]/g;

function escapeCell(value: unknown): string {
  if (value === null || value === undefined) return '';
  if (typeof value === 'boolean' || typeof value === 'number') {
    return String(value);
  }
  const str = typeof value === 'string' ? value : JSON.stringify(value);
  return str.replace(ESCAPE, ch => {
    if (ch === '\t') return '\\t';
    if (ch === '\n') return '\\n';
    if (ch === '\r') return '\\r';
    return '\\\\';
  });
}

/**
 * Render rows as a TSV string with a leading header row.
 *
 * `columns` defines the header and the projection: each `Row` is read by
 * column name, so the row objects can have extra fields that are simply
 * ignored. Missing fields render as empty cells.
 */
export function tsvFormat(
  columns: readonly string[],
  rows: ReadonlyArray<Record<string, unknown>>
): string {
  const header = columns.join('\t');
  if (rows.length === 0) return header;
  const body = rows
    .map(row => columns.map(col => escapeCell(row[col])).join('\t'))
    .join('\n');
  return `${header}\n${body}`;
}
