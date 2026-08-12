#!/usr/bin/env node
/**
 * Bridge: CDP HAR / network bodies → scrape session corpus (best of both skills).
 *
 * Flow A (CDP → scrape): ingest redacted HAR + bodies into session/cdp/ + extracts + sources.jsonl
 * Flow B (scrape → CDP packet): --export-packet writes extracts/bridge-handoff.json for graph-actionability
 *
 * Usage:
 *   node har-ingest.mjs --session-dir <scrapeSession> --har <file.har> [--bodies <network-bodies.json>]
 *   node har-ingest.mjs --session-dir <scrapeSession> --from-cdp-dir <.octocode/tmp/chrome-devtools/...>
 *   node har-ingest.mjs --session-dir <scrapeSession> --export-packet
 *   node har-ingest.mjs --session-dir <scrapeSession> --har <file.har> --filter api --domain walmart.com
 */
import { copyFile, writeFile, readFile } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { basename, join, resolve } from 'node:path';
import {
  takeArg,
  hasFlag,
  redactHarObject,
  compactHarEntry,
  filterHarRows,
  readJsonFile,
  appendJsonl,
  ensureDir,
  thinPageHint,
  patchAgentIndexForBridge,
  discoverCdpArtifacts,
  fileBytes,
} from './lib/bridge.mjs';

function usage(code = 2) {
  console.error(`Usage:
  har-ingest.mjs --session-dir <dir> --har <file.har> [--bodies <network-bodies.json>] [--filter all|failures|slow|api|json|domain:<host>] [--min-ms 1000] [--strip-bodies] [--no-redact]
  har-ingest.mjs --session-dir <dir> --from-cdp-dir <chrome-devtools-run-dir> [same filters]
  har-ingest.mjs --session-dir <dir> --export-packet

Writes under <session>/cdp/ and appends extracts/cdp-*.jsonl + sources.jsonl; patches AGENT_INDEX.json.`);
  process.exit(code);
}

const args = process.argv.slice(2);
if (hasFlag(args, '--help') || hasFlag(args, '-h')) usage(0);

const sessionDir = resolve(takeArg(args, '--session-dir'));
if (!sessionDir || sessionDir === resolve('')) usage();
if (!existsSync(sessionDir)) {
  console.log(JSON.stringify({ ok: false, error: `session-dir not found: ${sessionDir}` }));
  process.exit(1);
}

const exportPacket = hasFlag(args, '--export-packet');
const filter = takeArg(args, '--filter', 'all');
const minMs = Number(takeArg(args, '--min-ms', '1000')) || 1000;
const stripBodies = hasFlag(args, '--strip-bodies');
const noRedact = hasFlag(args, '--no-redact');
const domain = takeArg(args, '--domain', '');

await ensureDir(join(sessionDir, 'cdp'));
await ensureDir(join(sessionDir, 'extracts'));
await ensureDir(join(sessionDir, 'reports'));

const sourcesPath = join(sessionDir, 'sources.jsonl');
const existingSources = existsSync(sourcesPath)
  ? (await readFile(sourcesPath, 'utf8')).trim().split('\n').filter(Boolean).map((l) => JSON.parse(l))
  : [];
const thinHints = existingSources.map(thinPageHint).filter(Boolean);

async function writeExportPacket(extra = {}) {
  const agent = await readJsonFile(join(sessionDir, 'AGENT_INDEX.json'), {});
  const graph = await readJsonFile(join(sessionDir, 'graph/graph.json'), { nodes: [], edges: [] });
  const actionNodes = (graph.nodes || []).filter((n) => ['form', 'input', 'button', 'pagination', 'link'].includes(n.kind));
  const packet = {
    schemaVersion: 1,
    kind: 'octocode.scrape.cdpHandoff',
    createdAt: new Date().toISOString(),
    sessionDir,
    rootUrl: graph.rootUrl || existingSources[0]?.url || null,
    thinHints,
    warnings: agent.warnings || [],
    totals: agent.totals || {},
    graphPath: existsSync(join(sessionDir, 'graph/graph.json')) ? 'graph/graph.json' : null,
    selectors: actionNodes.map((n) => n.selector).filter(Boolean).slice(0, 40),
    actionNodes: actionNodes.slice(0, 40).map((n) => ({
      id: n.id,
      kind: n.kind,
      selector: n.selector || null,
      url: n.url || null,
      text: n.text || n.title || null,
      risk: n.risk || null,
    })),
    searchFirst: ['AGENT_INDEX.json', 'extracts/bridge-handoff.json', 'cdp/', 'extracts/cdp-network.jsonl', 'extracts/cdp-bodies.jsonl'],
    next: thinHints.length
      ? ['Run CDP network/HAR on the URL', 'har-ingest.mjs --from-cdp-dir <run>', 'corpus-run.mjs --regex on cdp/extracts']
      : ['Optional: graph-actionability-check.mjs --graph graph/graph.json'],
    ...extra,
  };
  const rel = 'extracts/bridge-handoff.json';
  await writeFile(join(sessionDir, rel), `${JSON.stringify(packet, null, 2)}\n`);
  await patchAgentIndexForBridge(sessionDir);
  return { packet, rel };
}

if (exportPacket && !takeArg(args, '--har') && !takeArg(args, '--from-cdp-dir')) {
  const { packet, rel } = await writeExportPacket();
  console.log(JSON.stringify({
    ok: true,
    flow: 'scrape→cdp-packet',
    sessionDir,
    handoff: rel,
    thinHints,
    selectors: packet.selectors.length,
    actionNodes: packet.actionNodes.length,
    next: packet.next,
  }, null, 2));
  process.exit(0);
}

let harPath = takeArg(args, '--har');
let bodiesPath = takeArg(args, '--bodies');
const fromCdpDir = takeArg(args, '--from-cdp-dir');

if (fromCdpDir) {
  const found = await discoverCdpArtifacts(fromCdpDir);
  harPath = harPath || found.har;
  bodiesPath = bodiesPath || found.bodies;
  if (!harPath && !bodiesPath) {
    console.log(JSON.stringify({ ok: false, error: 'no .har or network-bodies.json under --from-cdp-dir', fromCdpDir: resolve(fromCdpDir), files: found.files.map((f) => f.rel).slice(0, 30) }));
    process.exit(1);
  }
}

if (!harPath && !bodiesPath) usage();

const ingestedAt = new Date().toISOString();
const cdpFiles = [];
const networkRows = [];
const bodyRows = [];
const sourceRows = [];
let redactMeta = { headerRowsTouched: 0, cookiesRedacted: 0, entries: 0 };
let filteredCount = 0;

if (harPath) {
  const absHar = resolve(harPath);
  if (!existsSync(absHar)) {
    console.log(JSON.stringify({ ok: false, error: `HAR not found: ${absHar}` }));
    process.exit(1);
  }
  const har = JSON.parse(await readFile(absHar, 'utf8'));
  if (!noRedact) {
    const meta = redactHarObject(har, { stripBodies });
    redactMeta = {
      headerRowsTouched: meta.headerRowsTouched,
      cookiesRedacted: meta.cookiesRedacted,
      entries: (meta.entries || []).length,
    };
  } else {
    redactMeta.entries = (har.log?.entries || []).length;
  }

  const allRows = (har.log?.entries || []).map(compactHarEntry);
  const rows = filterHarRows(allRows, filter, { minMs, domain });
  filteredCount = rows.length;

  // Keep full redacted HAR on disk; extracts get filtered compact rows (pager-smart).
  const harOutRel = `cdp/${basename(absHar).replace(/\.har$/i, '')}.redacted.har`;
  await writeFile(join(sessionDir, harOutRel), `${JSON.stringify(har, null, 2)}\n`, { mode: 0o600 });
  cdpFiles.push(harOutRel);

  const compactRel = 'cdp/network-summary.json';
  await writeFile(join(sessionDir, compactRel), `${JSON.stringify({
    sourceHar: absHar,
    filter,
    minMs,
    domain: domain || null,
    allEntries: allRows.length,
    filteredEntries: rows.length,
    counts: {
      failures: allRows.filter((r) => r.failed).length,
      api: allRows.filter((r) => r.looksApi || r.looksJson).length,
      hosts: [...new Set(allRows.map((r) => r.host).filter(Boolean))].length,
    },
    rows,
  }, null, 2)}\n`);
  cdpFiles.push(compactRel);

  for (const row of rows) {
    networkRows.push({
      kind: 'cdp-network',
      ingestedAt,
      pageId: null,
      ...row,
      evidence: { file: harOutRel, summary: compactRel },
    });
  }

  sourceRows.push({
    pageId: `cdp-har-${Date.now().toString(36)}`,
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
}

if (bodiesPath) {
  const absBodies = resolve(bodiesPath);
  if (!existsSync(absBodies)) {
    console.log(JSON.stringify({ ok: false, error: `bodies file not found: ${absBodies}` }));
    process.exit(1);
  }
  const bodies = JSON.parse(await readFile(absBodies, 'utf8'));
  const list = Array.isArray(bodies) ? bodies : (bodies.bodies || bodies.entries || [bodies]);
  const outRel = 'cdp/network-bodies.json';
  // Shallow redact: never keep auth-looking string fields in wrapper metadata
  const safeList = list.map((b, i) => {
    const url = typeof b.url === 'string' ? b.url : '';
    let bodyText = b.body ?? b.text ?? '';
    if (typeof bodyText !== 'string') bodyText = JSON.stringify(bodyText);
    if (/password|authorization|\"token\"/i.test(bodyText) && bodyText.length > 500) {
      bodyText = '[REDACTED]';
    }
    const bodyFileRel = `cdp/body-${String(i + 1).padStart(3, '0')}.txt`;
    return { index: i, requestId: b.requestId || null, url, bodyText, bodyFileRel };
  });

  for (const item of safeList) {
    await writeFile(join(sessionDir, item.bodyFileRel), item.bodyText, { mode: 0o600 });
    cdpFiles.push(item.bodyFileRel);
    bodyRows.push({
      kind: 'cdp-body',
      ingestedAt,
      requestId: item.requestId,
      url: item.url,
      bodyFile: item.bodyFileRel,
      chars: item.bodyText.length,
      looksJson: /^\s*[\[{]/.test(item.bodyText),
      sample: item.bodyText.slice(0, 240),
    });
  }

  await writeFile(join(sessionDir, outRel), `${JSON.stringify(safeList.map(({ bodyText, ...rest }) => ({ ...rest, chars: bodyText.length })), null, 2)}\n`, { mode: 0o600 });
  cdpFiles.push(outRel);

  // Also copy original path reference note
  await copyFile(absBodies, join(sessionDir, 'cdp', `source-${basename(absBodies)}`)).catch(() => {});

  sourceRows.push({
    pageId: `cdp-bodies-${Date.now().toString(36)}`,
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
    bridge: { type: 'bodies', count: safeList.length },
  });
}

await appendJsonl(join(sessionDir, 'extracts/cdp-network.jsonl'), networkRows);
await appendJsonl(join(sessionDir, 'extracts/cdp-bodies.jsonl'), bodyRows);
await appendJsonl(sourcesPath, sourceRows);

const { packet, rel: handoffRel } = await writeExportPacket({
  ingested: {
    har: Boolean(harPath),
    bodies: Boolean(bodiesPath),
    cdpFiles,
    networkRows: networkRows.length,
    bodyRows: bodyRows.length,
    filter,
    filteredCount,
    redact: noRedact ? null : { headerRowsTouched: redactMeta.headerRowsTouched, cookiesRedacted: redactMeta.cookiesRedacted, entries: redactMeta.entries },
  },
});

await writeFile(join(sessionDir, 'reports/bridge-ingest.md'), `# CDP → Scrape Bridge Ingest

Ingested: ${ingestedAt}
Filter: ${filter}
HAR entries (filtered): ${networkRows.length}
Bodies: ${bodyRows.length}
Thin hints: ${thinHints.length ? thinHints.join('; ') : 'none'}

## Files
${cdpFiles.map((f) => `- \`${f}\``).join('\n') || '- (none)'}

## Next
1. \`corpus-run.mjs --session-dir ${sessionDir} --roots cdp,extracts --regex '<pattern>'\`
2. Prefer local proof over reopening Chrome when the API body is already under \`cdp/\`.
`);

await patchAgentIndexForBridge(sessionDir, { extracts: ['extracts/cdp-network.jsonl', 'extracts/cdp-bodies.jsonl'] });

console.log(JSON.stringify({
  ok: true,
  flow: 'cdp→scrape',
  sessionDir,
  thinHints,
  filter,
  filteredNetworkRows: networkRows.length,
  bodyRows: bodyRows.length,
  cdpFiles,
  extracts: {
    network: networkRows.length ? 'extracts/cdp-network.jsonl' : null,
    bodies: bodyRows.length ? 'extracts/cdp-bodies.jsonl' : null,
    handoff: handoffRel,
  },
  redact: noRedact ? null : { headerRowsTouched: redactMeta.headerRowsTouched, cookiesRedacted: redactMeta.cookiesRedacted, entries: redactMeta.entries },
  next: [
    `node skills/octocode-scraping/scripts/corpus-run.mjs --session-dir ${sessionDir} --roots cdp,extracts --regex 'product|itemId|offerId'`,
    ...(thinHints.length ? ['Thin page detected earlier — trust cdp/ bodies over text/*.clean.part-*.md'] : []),
  ],
  packetSelectors: packet.selectors.length,
}, null, 2));
