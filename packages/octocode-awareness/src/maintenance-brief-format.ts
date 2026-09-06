import type { BriefItem } from './maintenance-stale.js';

export function summarizeUtf8(value: string, maxBytes: number): string {
  const flat = value.replace(/\s+/g, ' ').trim();
  if (Buffer.byteLength(flat, 'utf8') <= maxBytes) return flat;
  const suffix = '...';
  const suffixBytes = Buffer.byteLength(suffix, 'utf8');
  let bytes = 0;
  let output = '';
  for (const character of flat) {
    const characterBytes = Buffer.byteLength(character, 'utf8');
    if (bytes + characterBytes + suffixBytes > maxBytes) break;
    output += character;
    bytes += characterBytes;
  }
  return output.trimEnd() + suffix;
}

function briefPath(file: string, workspacePath: string | null): string {
  const value = file.trim();
  if (!workspacePath) return value;
  const prefix = workspacePath.endsWith('/') ? workspacePath : `${workspacePath}/`;
  return value.startsWith(prefix) ? value.slice(prefix.length) : value;
}

function briefFiles(files: string[], workspacePath: string | null): string {
  const unique = [...new Set(files.map(file => briefPath(file, workspacePath)).filter(Boolean))].sort();
  if (unique.length === 0) return '';
  const first = summarizeUtf8(unique[0]!, 56);
  const more = unique.length > 1 ? ` (+${unique.length - 1})` : '';
  return `files ${unique.length}: ${first}${more}`;
}

function briefRoute(from: string, target: string): string[] {
  return [`from ${from}`, target];
}

export function notificationBriefText(params: {
  kind: string;
  from: string;
  target: string;
  files: string[];
  subject: string;
  body?: string;
  workspacePath: string | null;
  count?: number;
}): string {
  const kind = params.count && params.count > 1 ? `${params.kind} ×${params.count}` : params.kind;
  const parts = [
    `📨 ${kind}`,
    ...briefRoute(params.from, params.target),
    summarizeUtf8(params.subject, 72),
    briefFiles(params.files, params.workspacePath),
  ].filter(Boolean);
  const bodySuffix = params.body ? ` — ${summarizeUtf8(params.body, 60)}` : '';
  return `${parts.join(' · ')}${bodySuffix}`;
}

export function compactBriefItems(items: BriefItem[]): BriefItem[] {
  const grouped = new Map<string, BriefItem & { duplicateCount: number }>();
  for (const item of items) {
    const key = `${item.kind}\0${item.text}`;
    const existing = grouped.get(key);
    if (existing) {
      existing.duplicateCount += 1;
      existing.importance = Math.max(existing.importance ?? 0, item.importance ?? 0) || undefined;
      continue;
    }
    grouped.set(key, { ...item, duplicateCount: 1 });
  }
  return [...grouped.values()].map(({ duplicateCount, ...item }) => duplicateCount > 1
    ? { ...item, text: `${item.text} (duplicate ×${duplicateCount})` }
    : item);
}
