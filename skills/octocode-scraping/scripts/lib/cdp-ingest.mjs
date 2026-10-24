// har-ingest capture readers: each turns one CDP artifact (HAR, network bodies, page snapshot
// or DOM check) into redacted files under <session>/cdp/ plus extract and sources.jsonl rows.
import { writeFile, readFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { existsSync } from 'node:fs';
import { basename, join, resolve } from 'node:path';
import {
  redactHarObject,
  redactSecretsInText,
  redactSecretsInJson,
  redactUrl,
  compactHarEntry,
  filterHarRows,
  fileBytes,
} from './bridge.mjs';
import { fail } from './fail.mjs';

// Content-addressed ids: re-ingesting the same capture replaces its rows instead of duplicating them,
// and two different captures never overwrite each other's files.
const ingestId = (kind, text) => `cdp-${kind}-${createHash('sha256').update(text).digest('hex').slice(0, 12)}`;

/** Ingests a HAR file; returns its redaction counts and the number of rows that passed the filter. */
// Redacts the HAR in place unless noRedact; returns the counts either way.
function redactHar(har, noRedact, stripBodies) {
  if (noRedact) return { headerRowsTouched: 0, cookiesRedacted: 0, entries: (har.log?.entries || []).length };
  const meta = redactHarObject(har, { stripBodies });
  return {
    headerRowsTouched: meta.headerRowsTouched,
    cookiesRedacted: meta.cookiesRedacted,
    entries: (meta.entries || []).length,
  };
}

const harCounts = (allRows) => ({
  failures: allRows.filter((r) => r.failed).length,
  api: allRows.filter((r) => r.looksApi || r.looksJson).length,
  hosts: new Set(allRows.map((r) => r.host).filter(Boolean)).size,
});

export async function ingestHar(harPath, { sessionDir, ingestedAt, noRedact, existingSources, filter, minMs, domain, stripBodies }, { cdpFiles, networkRows, sourceRows }) {
  const absHar = resolve(harPath);
  if (!existsSync(absHar)) {
    fail('notFound', `HAR not found: ${absHar}`, 1);
  }
  const harText = await readFile(absHar, 'utf8');
  const har = JSON.parse(harText);
  const harId = ingestId('har', harText);
  const redactMeta = redactHar(har, noRedact, stripBodies);

  const allRows = (har.log?.entries || []).map(compactHarEntry);
  const rows = filterHarRows(allRows, filter, { minMs, domain });

  // Keep full redacted HAR on disk; extracts get filtered compact rows (pager-smart).
  const harOutRel = `cdp/${harId}.${noRedact ? 'raw' : 'redacted'}.har`;
  await writeFile(join(sessionDir, harOutRel), `${JSON.stringify(har, null, 2)}\n`, { mode: 0o600 });
  cdpFiles.push(harOutRel);

  const compactRel = `cdp/${harId}-network-summary.json`;
  await writeFile(join(sessionDir, compactRel), `${JSON.stringify({
    sourceHar: absHar,
    filter,
    minMs,
    domain: domain || null,
    allEntries: allRows.length,
    filteredEntries: rows.length,
    counts: harCounts(allRows),
    rows,
  }, null, 2)}\n`);
  cdpFiles.push(compactRel);

  for (const row of rows) {
    networkRows.push({
      kind: 'cdp-network',
      ingestId: harId,
      ingestedAt,
      pageId: null,
      ...row,
      evidence: { file: harOutRel, summary: compactRel },
    });
  }

  sourceRows.push({
    pageId: harId,
    url: rows[0]?.url || existingSources[0]?.url || absHar,
    route: 'bridge:har-ingest',
    provider: 'cdp-bridge',
    status: 200,
    ok: true,
    contentType: 'application/json',
    fetchedAt: ingestedAt,
    raw: harOutRel,
    text: compactRel,
    textParts: [],
    rawTruncated: false,
    rawBytes: await fileBytes(join(sessionDir, harOutRel)),
    textTruncated: false,
    textBytes: await fileBytes(join(sessionDir, compactRel)),
    cleanTextBytes: await fileBytes(join(sessionDir, compactRel)),
    antCreditsCost: null,
    error: null,
    providerStatus: null,
    providerDetail: null,
    targetLikelyError: null,
    bridge: { type: 'har', filter, filteredEntries: rows.length, allEntries: allRows.length },
  });
  return { redactMeta, filteredCount: rows.length };
}

/** Ingests a network-bodies JSON file; every body and URL is redacted unless noRedact. */
export async function ingestBodies(bodiesPath, { sessionDir, ingestedAt, noRedact, existingSources }, { cdpFiles, bodyRows, sourceRows }) {
  const absBodies = resolve(bodiesPath);
  if (!existsSync(absBodies)) {
    fail('notFound', `bodies file not found: ${absBodies}`, 1);
  }
  const bodiesText = await readFile(absBodies, 'utf8');
  const bodies = JSON.parse(bodiesText);
  const bodiesId = ingestId('bodies', bodiesText);
  const list = Array.isArray(bodies) ? bodies : (bodies.bodies || bodies.entries || [bodies]);
  const outRel = `cdp/${bodiesId}-network-bodies.json`;
  // Mask credential values in every body and URL; the rest of each body stays as evidence.
  const safeList = list.map((b, i) => {
    const rawUrl = typeof b.url === 'string' ? b.url : '';
    const url = noRedact ? rawUrl : redactUrl(rawUrl);
    let bodyText = b.body ?? b.text ?? '';
    if (typeof bodyText !== 'string') bodyText = JSON.stringify(bodyText);
    if (!noRedact) bodyText = redactSecretsInText(bodyText);
    const bodyFileRel = `cdp/${bodiesId}-body-${String(i + 1).padStart(3, '0')}.txt`;
    return { index: i, requestId: b.requestId || null, url, bodyText, bodyFileRel };
  });

  for (const item of safeList) {
    await writeFile(join(sessionDir, item.bodyFileRel), item.bodyText, { mode: 0o600 });
    cdpFiles.push(item.bodyFileRel);
    bodyRows.push({
      kind: 'cdp-body',
      ingestId: bodiesId,
      ingestedAt,
      requestId: item.requestId,
      url: item.url,
      bodyFile: item.bodyFileRel,
      chars: item.bodyText.length,
      looksJson: /^\s*[[{]/.test(item.bodyText),
      sample: item.bodyText.slice(0, 240),
    });
  }

  await writeFile(join(sessionDir, outRel), `${JSON.stringify(safeList.map(({ bodyText, ...rest }) => ({ ...rest, chars: bodyText.length })), null, 2)}\n`, { mode: 0o600 });
  cdpFiles.push(outRel);

  sourceRows.push({
    pageId: bodiesId,
    url: safeList[0]?.url || existingSources[0]?.url || absBodies,
    route: 'bridge:har-ingest',
    provider: 'cdp-bridge',
    status: 200,
    ok: true,
    contentType: 'application/json',
    fetchedAt: ingestedAt,
    raw: outRel,
    text: safeList[0]?.bodyFileRel || outRel,
    textParts: safeList.map((s) => s.bodyFileRel),
    rawTruncated: false,
    rawBytes: await fileBytes(join(sessionDir, outRel)),
    textTruncated: false,
    textBytes: safeList.reduce((n, s) => n + s.bodyText.length, 0),
    cleanTextBytes: safeList.reduce((n, s) => n + s.bodyText.length, 0),
    antCreditsCost: null,
    error: null,
    providerStatus: null,
    providerDetail: null,
    targetLikelyError: null,
    bridge: { type: 'bodies', count: safeList.length, sourceFile: absBodies },
  });
}

function redactCapture(original, noRedact) {
  if (noRedact) return original;
  const data = redactSecretsInJson(original);
  if (typeof data.location === 'string') data.location = redactUrl(data.location);
  if (typeof data.url === 'string') data.url = redactUrl(data.url);
  return data;
}

const snapshotRow = (data, base, { file, fallbackUrl }) => ({
  kind: 'cdp-page-snapshot',
  ...base,
  url: data.url || fallbackUrl,
  refCount: Object.keys(data.refs || {}).length,
  refs: Object.entries(data.refs || {}).slice(0, 60).map(([ref, value]) => ({ ref, role: value.role || null, name: value.name || null })),
  evidence: { file },
});

const domCheckRow = (data, base, { file, fallbackUrl }) => ({
  kind: 'cdp-dom-check',
  ...base,
  url: data.location || fallbackUrl,
  found: Boolean(data.found),
  action: data.action || null,
  operation: data.operation || null,
  role: data.role || null,
  name: data.accessibleNameGuess || data.text || null,
  canOperate: data.canOperate ?? null,
  recoveredFromStaleRef: Boolean(data.recoveredFromStaleRef),
  evidence: { file },
});

/** Ingests one page-snapshot.json or dom-check.json capture. */
export async function ingestCapture(capturePath, { sessionDir, ingestedAt, noRedact, existingSources }, { cdpFiles, captureRows, sourceRows }) {
  const absCapture = resolve(capturePath);
  const name = basename(absCapture);
  const captureText = await readFile(absCapture, 'utf8');
  const original = JSON.parse(captureText);
  const captureId = ingestId('capture', captureText);
  const data = redactCapture(original, noRedact);
  const outRel = `cdp/${captureId}-${name}`;
  await writeFile(join(sessionDir, outRel), `${JSON.stringify(data, null, 2)}\n`, { mode: 0o600 });
  cdpFiles.push(outRel);
  const toRow = /^page-snapshot\.json$/i.test(name) ? snapshotRow : domCheckRow;
  const row = toRow(data, { ingestId: captureId, ingestedAt }, { file: outRel, fallbackUrl: existingSources[0]?.url || null });
  captureRows.push(row);
  sourceRows.push({
    pageId: captureId,
    url: row.url || absCapture,
    route: 'bridge:cdp-capture',
    provider: 'cdp-bridge',
    status: 200,
    ok: true,
    contentType: 'application/json',
    fetchedAt: ingestedAt,
    raw: outRel,
    text: 'extracts/cdp-captures.jsonl',
    textParts: [],
    rawTruncated: false,
    rawBytes: await fileBytes(join(sessionDir, outRel)),
    textTruncated: false,
    textBytes: JSON.stringify(row).length,
    cleanTextBytes: JSON.stringify(row).length,
    antCreditsCost: null,
    error: null,
    providerStatus: null,
    providerDetail: null,
    targetLikelyError: null,
    bridge: { type: row.kind },
  });
}
