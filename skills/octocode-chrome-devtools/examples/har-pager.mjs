#!/usr/bin/env node
import { readFileSync } from 'fs';
import { resolve } from 'path';

const argv = process.argv.slice(2);
const harArg = argv.find(arg => !arg.startsWith('--'));
const getArg = (flag, def) => {
  const index = argv.indexOf(flag);
  return index === -1 ? def : argv[index + 1] ?? def;
};
const hasFlag = (flag) => argv.includes(flag);

if (!harArg || hasFlag('--help')) {
  console.error(`Usage: node har-pager.mjs <file.har> [--page 1] [--page-size 25] [--filter all|failures|slow|domain:<host>] [--min-ms 1000] [--format text|json]\n\nReads a HAR 1.2 file and prints one compact page for agent review.`);
  process.exit(harArg ? 0 : 1);
}

const page = Math.max(1, Number.parseInt(getArg('--page', '1'), 10));
const pageSize = Math.max(1, Math.min(200, Number.parseInt(getArg('--page-size', '25'), 10)));
const filter = getArg('--filter', 'all');
const minMs = Math.max(0, Number.parseInt(getArg('--min-ms', '1000'), 10));
const format = getArg('--format', 'text');
const harPath = resolve(process.cwd(), harArg);
const har = JSON.parse(readFileSync(harPath, 'utf8'));
const entries = Array.isArray(har.log?.entries) ? har.log.entries : [];

function hostOf(raw) {
  try { return new URL(raw).hostname; } catch { return ''; }
}

function compactEntry(entry, index) {
  const request = entry.request ?? {};
  const response = entry.response ?? {};
  return {
    index,
    startedDateTime: entry.startedDateTime,
    method: request.method,
    url: request.url,
    host: hostOf(request.url),
    status: response.status,
    mimeType: response.content?.mimeType ?? '',
    ms: Math.round(entry.time ?? 0),
    bodySize: response.bodySize ?? response.content?.size ?? -1,
    type: entry._resourceType ?? '',
    failed: Boolean(entry._failed || response.status >= 400 || response.status === 0),
    errorText: entry._errorText ?? null,
    blockedReason: entry._blockedReason ?? null,
  };
}

let rows = entries.map(compactEntry);
if (filter === 'failures') {
  rows = rows.filter(row => row.failed);
} else if (filter === 'slow') {
  rows = rows.filter(row => row.ms >= minMs);
} else if (filter.startsWith('domain:')) {
  const host = filter.slice('domain:'.length).toLowerCase();
  rows = rows.filter(row => row.host.toLowerCase().includes(host));
}

const totalPages = Math.max(1, Math.ceil(rows.length / pageSize));
const start = (page - 1) * pageSize;
const pageRows = rows.slice(start, start + pageSize);
const aggregate = {
  harPath,
  filter,
  page,
  pageSize,
  totalRows: rows.length,
  totalPages,
  allEntries: entries.length,
  counts: {
    failures: entries.filter((entry, index) => compactEntry(entry, index).failed).length,
    slow: entries.filter(entry => (entry.time ?? 0) >= minMs).length,
    hosts: new Set(entries.map(entry => hostOf(entry.request?.url)).filter(Boolean)).size,
  },
  pageRows,
};

if (format === 'json') {
  console.log(JSON.stringify(aggregate, null, 2));
} else {
  console.log(`[HAR_PAGE] file=${harPath}`);
  console.log(`[HAR_PAGE] filter=${filter} page=${page}/${totalPages} pageSize=${pageSize} rows=${rows.length} allEntries=${entries.length}`);
  console.log(`[HAR_METRIC] failures=${aggregate.counts.failures} slowOver${minMs}ms=${aggregate.counts.slow} hosts=${aggregate.counts.hosts}`);
  for (const row of pageRows) {
    const status = row.failed ? 'FAIL' : 'OK';
    console.log(`[HAR_ENTRY] #${row.index} ${status} ${row.status} ${row.method} ${row.ms}ms ${row.type} ${row.url}`);
    if (row.errorText || row.blockedReason) {
      console.log(`[HAR_DETAIL] #${row.index} error=${row.errorText ?? ''} blocked=${row.blockedReason ?? ''}`);
    }
  }
  if (page < totalPages) console.log(`[HAR_NEXT] node ${process.argv[1]} ${harPath} --filter ${filter} --page ${page + 1} --page-size ${pageSize} --min-ms ${minMs}`);
}
