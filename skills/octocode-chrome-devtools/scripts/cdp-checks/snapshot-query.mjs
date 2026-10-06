#!/usr/bin/env node
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
const args = process.argv.slice(2);
const arg = (flag, fallback) => args.includes(flag) ? args[args.indexOf(flag) + 1] : fallback;
if (args.includes('--help') || args.includes('-h')) {
  console.log('Usage: snapshot-query.mjs --file <page-snapshot.json> [--page 1] [--limit 60] [--view refs|outline]\nPages saved evidence without recapturing a changing page.');
  process.exit(0);
}
try {
  if (!arg('--file')) throw new Error('--file is required');
  const file = resolve(arg('--file'));
  const page = Number(arg('--page', '1')), limit = Number(arg('--limit', '60'));
  const view = arg('--view', 'refs');
  if (!Number.isSafeInteger(page) || page < 1 || !Number.isSafeInteger(limit) || limit < 1 || limit > 300 || !['refs', 'outline'].includes(view)) throw new Error('Invalid page, limit, or view');
  const data = JSON.parse(readFileSync(file, 'utf8'));
  const rows = view === 'refs' ? Object.entries(data.refs).map(([ref, row]) => ({ ref, ...row })) : data.outline;
  if (!Array.isArray(rows)) throw new Error('Requested view unavailable in this capture');
  const offset = (page - 1) * limit;
  const hasMore = offset + limit < rows.length;
  console.log(JSON.stringify({ file, url: data.url, title: data.title, view, page, limit, totalRows: rows.length, rows: rows.slice(offset, offset + limit),
    ...(hasMore ? { next: { continue: { command: process.execPath, args: [fileURLToPath(import.meta.url), '--file', file, '--page', String(page + 1), '--limit', String(limit), '--view', view] } } } : {}),
  }, null, 2));
} catch (error) { console.error('[SNAPSHOT_QUERY] ' + error.message); process.exitCode = 1; }
