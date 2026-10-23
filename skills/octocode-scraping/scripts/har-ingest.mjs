#!/usr/bin/env node
/**
 * Bridge: CDP HAR / network bodies → scrape session corpus (best of both skills).
 *
 * Flow A (CDP → scrape): ingest redacted HAR + bodies into session/cdp/ + extracts + sources.jsonl
 * Flow B (scrape → CDP packet): --export-packet writes extracts/bridge-handoff.json with the actionable graph nodes
 *
 * Usage:
 *   node har-ingest.mjs --session-dir <scrapeSession> --har <file.har> [--bodies <network-bodies.json>]
 *   node har-ingest.mjs --session-dir <scrapeSession> --from-cdp-dir <.octocode/octocode-chrome-devtools/tmp/...>
 *   node har-ingest.mjs --session-dir <scrapeSession> --export-packet
 *   node har-ingest.mjs --session-dir <scrapeSession> --har <file.har> --filter api --domain walmart.com
 */
import { writeFile, readFile } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  takeArg,
  hasFlag,
  readJsonFile,
  replaceJsonl,
  ensureDir,
  thinPageHint,
  patchAgentIndexForBridge,
  discoverCdpArtifacts,
} from './lib/bridge.mjs';
import { ingestBodies, ingestCapture, ingestHar } from './lib/cdp-ingest.mjs';
import { fail } from './lib/fail.mjs';


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
  fail('sessionNotFound', `session-dir not found: ${sessionDir}`, 1);
}

const exportPacket = hasFlag(args, '--export-packet');
const filter = takeArg(args, '--filter', 'all');
const minMs = Number(takeArg(args, '--min-ms', '1000'));
if (!Number.isFinite(minMs) || minMs < 0) fail('invalidArguments', '--min-ms must be a non-negative number', 2);
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
      : [`Optional: node ${fileURLToPath(new URL('./graph-navigate.mjs', import.meta.url))} --session-dir ${sessionDir} --view actionNodes`],
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
let capturePaths = [];

if (fromCdpDir) {
  const found = await discoverCdpArtifacts(fromCdpDir);
  harPath = harPath || found.har;
  bodiesPath = bodiesPath || found.bodies;
  capturePaths = found.captures;
  if (!harPath && !bodiesPath && !capturePaths.length) {
    fail('notFound', 'no .har, network-bodies.json, page-snapshot.json, or dom-check.json under --from-cdp-dir', 1, { fromCdpDir: resolve(fromCdpDir), files: found.files.map((f) => f.rel).slice(0, 30) });
  }
}

if (!harPath && !bodiesPath && !capturePaths.length) usage();

const ingestedAt = new Date().toISOString();
const corpusRun = fileURLToPath(new URL('./corpus-run.mjs', import.meta.url));
const cdpFiles = [];
const networkRows = [];
const bodyRows = [];
const captureRows = [];
const sourceRows = [];
let redactMeta = { headerRowsTouched: 0, cookiesRedacted: 0, entries: 0 };
let filteredCount = 0;
const ingestContext = { sessionDir, ingestedAt, noRedact, existingSources };
const ingestOut = { cdpFiles, networkRows, bodyRows, captureRows, sourceRows };

if (harPath) {
  ({ redactMeta, filteredCount } = await ingestHar(harPath, { ...ingestContext, filter, minMs, domain, stripBodies }, ingestOut));
}

if (bodiesPath) await ingestBodies(bodiesPath, ingestContext, ingestOut);

for (const capturePath of capturePaths) await ingestCapture(capturePath, ingestContext, ingestOut);

const ingestIds = new Set(sourceRows.map((row) => row.pageId));
await replaceJsonl(join(sessionDir, 'extracts/cdp-network.jsonl'), networkRows, (row) => ingestIds.has(row.ingestId));
await replaceJsonl(join(sessionDir, 'extracts/cdp-bodies.jsonl'), bodyRows, (row) => ingestIds.has(row.ingestId));
await replaceJsonl(join(sessionDir, 'extracts/cdp-captures.jsonl'), captureRows, (row) => ingestIds.has(row.ingestId));
await replaceJsonl(sourcesPath, sourceRows, (row) => ingestIds.has(row.pageId));

const { packet, rel: handoffRel } = await writeExportPacket({
  ingested: {
    har: Boolean(harPath),
    bodies: Boolean(bodiesPath),
    captures: captureRows.length,
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
DOM/snapshot captures: ${captureRows.length}
Thin hints: ${thinHints.length ? thinHints.join('; ') : 'none'}

## Files
${cdpFiles.map((f) => `- \`${f}\``).join('\n') || '- (none)'}

## Next
1. \`node ${corpusRun} --session-dir ${sessionDir} --roots cdp,extracts --regex '<pattern>'\`
2. Prefer local proof over reopening Chrome when the API body is already under \`cdp/\`.
`);

await patchAgentIndexForBridge(sessionDir, { extracts: ['extracts/cdp-network.jsonl', 'extracts/cdp-bodies.jsonl', 'extracts/cdp-captures.jsonl'] });

console.log(JSON.stringify({
  ok: true,
  flow: 'cdp→scrape',
  sessionDir,
  thinHints,
  filter,
  filteredNetworkRows: networkRows.length,
  bodyRows: bodyRows.length,
  captureRows: captureRows.length,
  cdpFiles,
  extracts: {
    network: networkRows.length ? 'extracts/cdp-network.jsonl' : null,
    bodies: bodyRows.length ? 'extracts/cdp-bodies.jsonl' : null,
    captures: captureRows.length ? 'extracts/cdp-captures.jsonl' : null,
    handoff: handoffRel,
  },
  redact: noRedact ? null : { headerRowsTouched: redactMeta.headerRowsTouched, cookiesRedacted: redactMeta.cookiesRedacted, entries: redactMeta.entries },
  next: [
    `node ${corpusRun} --session-dir ${sessionDir} --roots cdp,extracts --regex 'product|itemId|offerId'`,
    ...(thinHints.length ? ['Thin page detected earlier — trust cdp/ bodies over text/*.clean.part-*.md'] : []),
  ],
  packetSelectors: packet.selectors.length,
}, null, 2));
