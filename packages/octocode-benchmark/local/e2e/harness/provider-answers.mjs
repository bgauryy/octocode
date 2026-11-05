// Recorded provider answers against the built binary (tool-health/recorded-*.mjs).
//
//   node provider-answers.mjs                the shape check, hermetic: replays each
//                                            recording; a field the provider sends
//                                            must reach the output or be declared
//                                            `unread`, and the output must meet the
//                                            published contract
//   node provider-answers.mjs --record [id,...]
//                                            re-record live: `dev.mjs answers:record`
//   node provider-answers.mjs --accept-unread <id,...>
//                                            declare a case's current gaps `unread`,
//                                            for a reviewer to read in the diff
//
// OCTOCODE_NATIVE_BIN with NODE_ENV=test runs any other binary, an older pin too.
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { checks, spawnCli, writeResults } from './mcp-client.mjs';
import { contractValidators } from './contract.mjs';
import { startRegistryProxy } from './tool-health/mock-registry-proxy.mjs';
import { REGISTRY_HOSTS } from './tool-health/mock-routes-registry.mjs';
import { hermeticEnv } from './tool-health/surfaces.mjs';
import { casePath, hasCase, hostsOf, readCase, replayer, requestKey, served, writeCase } from './tool-health/recorded-answers.mjs';
import { declaredUnread, fieldsOf, outputText, shapeDrift, shapeOf, trimmed, uncarried } from './tool-health/recorded-shape.mjs';
import { EXTRA_HOSTS, PLAN } from './tool-health/recorded-plan.mjs';

const args = process.argv.slice(2);
const ACCEPT = args.includes('--accept-unread');
const MODE = ACCEPT ? 'check' : args.includes('--record') ? 'record' : 'check';
const named = args.filter(arg => !arg.startsWith('--')).flatMap(arg => arg.split(',')).filter(Boolean);
const planned = named.length ? PLAN.filter(entry => named.some(id => entry.id === id || entry.id.startsWith(`${id}/`))) : PLAN;
if (named.length && !planned.length) throw new Error(`no recording named ${named.join(', ')}`);

const KEPT_HEADERS = new Set(['content-type', 'link', 'etag', 'cache-control', 'retry-after']);
const DROPPED_REQUEST_HEADERS = new Set(['host', 'connection', 'content-length', 'accept-encoding', 'proxy-connection']);
const { check, skip, summary, results } = checks(`provider-answers-${MODE}`);
const notes = [], gapsByCase = {};
const say = line => process.stdout.write(`${line}\n`);
const note = text => { notes.push(text); say(`NOTE ${text}`); };

function githubToken() {
  const inherited = process.env.GITHUB_TOKEN || process.env.GH_TOKEN;
  if (inherited) return inherited;
  const gh = spawnSync('gh', ['auth', 'token'], { encoding: 'utf8', timeout: 10_000 });
  return gh.status === 0 ? gh.stdout.trim() : '';
}

/** One tool call, its whole response, with `answer` standing in for the providers. */
async function runCall(entry, { hosts, answer, token }) {
  const tmp = fs.mkdtempSync(path.join(process.platform === 'darwin' ? '/tmp' : os.tmpdir(), 'octocode-answers-'));
  const home = path.join(tmp, 'home');
  fs.mkdirSync(home, { recursive: true });
  const proxy = await startRegistryProxy(tmp, { hosts, answer });
  try {
    const env = { ...hermeticEnv(home, 'https://api.github.com', MODE !== 'check', tmp), GITHUB_API_URL: 'https://api.github.com', HTTPS_PROXY: proxy.proxyUrl, https_proxy: proxy.proxyUrl, SSL_CERT_FILE: proxy.caFile, SSL_CERT_DIR: '', GITHUB_TOKEN: token };
    const outputs = [];
    let input = { queries: [entry.query], responseChars: 1_000_000 };
    while (input) {
      const run = await spawnCli([entry.tool, JSON.stringify(input), '--json'], { cwd: tmp, env });
      let output;
      try { output = JSON.parse(run.stdout); } catch { return { outputs, failure: `not JSON, exit ${run.status}: ${(run.stdout + run.stderr).slice(0, 200)}` }; }
      outputs.push(output);
      // A split response finishes; the provider's next page is another request, not this case.
      input = output.responsePagination?.next?.input;
    }
    return { outputs, denied: proxy.denied };
  } finally {
    await proxy.close();
    fs.rmSync(tmp, { recursive: true, force: true });
  }
}

/** The provider's next page, from a Link header. */
const nextFrom = link => /<([^>]+)>;\s*rel="next"/.exec(link ?? '')?.[1];
const EMAIL = /[A-Za-z0-9._%+-]+@[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)+/g;
/** Public, but a person's: the shape stays and the address goes. */
const withoutAddresses = text => text.replace(EMAIL, 'person@example.test');

/** A real answer with nothing kept but what a parser reads: no cookie, no request header. */
async function liveAnswer(request, sentHeaders = {}) {
  const forwarded = Object.fromEntries(Object.entries(sentHeaders).filter(([name]) => !DROPPED_REQUEST_HEADERS.has(name)));
  const response = await fetch(request.url, { method: request.method, headers: { ...forwarded, 'accept-encoding': 'identity' }, body: request.body, redirect: 'manual', signal: AbortSignal.timeout(60_000) });
  const bytes = Buffer.from(await response.arrayBuffer());
  const headers = [...response.headers].filter(([name]) => KEPT_HEADERS.has(name));
  const next = nextFrom(response.headers.get('link'));
  const common = { status: response.status, headers, ...(next && { next }), digest: `sha256:${createHash('sha256').update(bytes).digest('hex')}` };
  let text;
  try { text = new TextDecoder('utf-8', { fatal: true }).decode(bytes); } catch { return { ...common, base64: bytes.toString('base64') }; }
  try { return { ...common, body: JSON.parse(withoutAddresses(text)) }; } catch { return { ...common, body: withoutAddresses(text) }; }
}

const parsedBody = body => { if (!body) return undefined; try { return JSON.parse(body); } catch { return body; } };

/** Every answer the call gets, live, as the store keeps it. */
async function liveRun(entry, token) {
  const exchanges = new Map();
  const failures = [];
  const answer = async (req, url, sent) => {
    const request = { method: req.method, url: url.href, ...(req.headers.accept && { accept: req.headers.accept }), ...(sent && { body: parsedBody(sent) }) };
    try {
      const exchange = await liveAnswer({ method: req.method, url: url.href, ...(sent && { body: sent }) }, req.headers);
      exchanges.set(requestKey(request), { request, ...exchange });
      return served(exchange);
    } catch (error) {
      failures.push(`${request.url}: ${error.message}`);
      return { status: 502, body: { message: error.message } };
    }
  };
  const run = await runCall(entry, { hosts: [...REGISTRY_HOSTS, ...EXTRA_HOSTS], answer, token });
  for (const also of entry.also ?? []) exchanges.set(requestKey(also), { request: also, ...(await liveAnswer(also, { 'user-agent': 'octocode-provider-answers', accept: also.accept })), alternate: true });
  if (run.denied?.length) failures.push(`host not in EXTRA_HOSTS: ${run.denied.join(', ')}`);
  return { ...run, exchanges: [...exchanges.values()], failures };
}

const shapeKept = (entry, exchange, pinned) => {
  const fields = exchange.body;
  return fields && typeof fields === 'object' ? trimmed(fields, { maps: entry.maps, pinned, whole: entry.whole }) : fields;
};

const sensitive = (exchange, token) => {
  const text = JSON.stringify(exchange);
  if (token && text.includes(token)) return 'holds the token it was recorded with';
  return /"private":\s*true/.test(text) ? 'holds a private repository' : null;
};

async function record(entry, token) {
  const run = await liveRun(entry, token);
  const unsafe = run.exchanges.map(exchange => sensitive(exchange, token)).find(Boolean);
  const failed = run.exchanges.filter(exchange => exchange.status >= 500 || exchange.status === 429 || exchange.status === 401 || exchange.status === 403);
  const pinned = [entry.query.version].filter(Boolean);
  const lost = run.exchanges.flatMap(exchange => {
    if (!exchange.body || typeof exchange.body !== 'object') return [];
    const kept = shapeDrift(shapeOf(exchange.body, entry.maps), shapeOf(shapeKept(entry, exchange, pinned), entry.maps));
    return kept.failures.map(text => `trimming lost ${text}: ${exchange.request.url}`);
  });
  if (lost.length) { check(`${entry.id}.recorded`, false, lost[0]); return; }
  if (!check(`${entry.id}.recorded`, !unsafe && !run.failure && !run.failures.length && !failed.length && run.exchanges.length > 0, unsafe ?? run.failure ?? run.failures[0] ?? (failed[0] && `${failed[0].status} ${failed[0].request.url}`) ?? '')) return;
  const before = hasCase(entry.id) ? fs.readFileSync(casePath(entry.id), 'utf8') : null;
  writeCase(entry.id, {
    case: entry.id, tool: entry.tool, query: entry.query, recordedAt: new Date().toISOString().slice(0, 10),
    // What the tool does not show survives a refresh: a new field still needs its decision.
    unread: hasCase(entry.id) ? readCase(entry.id).unread : [],
    exchanges: run.exchanges.map(exchange => ({ ...exchange, ...(exchange.body !== undefined && { body: shapeKept(entry, exchange, pinned) }) })),
  });
  await replaysAsAsked(entry, run, before);
}

/** A trimmed answer that changes what the tool asks next has lost more than text: keep the old file. */
async function replaysAsAsked(entry, run, before) {
  const recording = readCase(entry.id);
  const { answer, asked, unrecorded } = replayer(recording);
  await runCall(entry, { hosts: [...new Set([...REGISTRY_HOSTS, ...hostsOf(recording)])], answer, token: 'recorded-answers-fake-token' });
  const live = run.exchanges.filter(exchange => !exchange.alternate).map(exchange => requestKey(exchange.request));
  const differs = [...live.filter(key => !asked.has(key)), ...unrecorded];
  if (check(`${entry.id}.replays`, !differs.length, differs[0]?.slice(0, 200) ?? '')) return;
  if (before === null) fs.rmSync(casePath(entry.id));
  else fs.writeFileSync(casePath(entry.id), before);
}


const { validateOutput } = contractValidators();

/** The published contract and the field accounting, on one replayed recording. */
async function shapeCheck(entry) {
  if (!check(`${entry.id}.stored`, hasCase(entry.id), hasCase(entry.id) ? '' : 'no recording: run dev.mjs answers:record')) return;
  const recording = readCase(entry.id);
  const { answer, unrecorded } = replayer(recording);
  const run = await runCall(entry, { hosts: [...new Set([...REGISTRY_HOSTS, ...hostsOf(recording)])], answer, token: 'recorded-answers-fake-token' });
  if (!check(`${entry.id}.ran`, !run.failure, run.failure ?? '')) return;
  check(`${entry.id}.requests`, !unrecorded.length, unrecorded[0] ?? '');
  const rows = run.outputs.flatMap(output => output.results ?? []);
  const codes = rows.map(row => row.data?.errorCode ?? row.errorCode).filter(Boolean);
  const expected = entry.errorCode ? codes.includes(entry.errorCode) : !codes.length;
  const errors = rows.map(row => row.data?.error ?? row.error).filter(Boolean);
  check(`${entry.id}.result`, expected, expected ? '' : `${codes.join(',')}: ${errors.join(' ').slice(0, 120)}`);
  const invalid = run.outputs.map(output => validateOutput(entry.tool, output)).find(Boolean);
  check(`${entry.id}.contract`, !invalid, invalid ?? '');
  fieldsCarried(entry, recording, run.outputs.map(outputText).join('\n'));
}

/** Each field the provider sent reached `text`, or is declared `unread`. */
function fieldsCarried(entry, recording, text) {
  const answers = recording.exchanges.filter(exchange => !exchange.alternate && exchange.status < 400).map(fieldsOf).filter(fields => fields !== undefined);
  const unique = [...new Set(answers.flatMap(fields => uncarried(fields, text, { unread: recording.unread, maps: entry.maps })))];
  if (unique.length) gapsByCase[entry.id] = unique;
  if (ACCEPT && unique.length) {
    const open = answers.flatMap(fields => uncarried(fields, text, { maps: entry.maps }));
    writeCase(entry.id, { ...recording, unread: declaredUnread([...new Set(open)], answers, entry.maps) });
    note(`${entry.id}: unread now declares ${unique.length} more field(s)`);
    return;
  }
  check(`${entry.id}.carried`, !unique.length, unique.length ? `${unique.length} field(s) neither shown nor declared unread, first: ${unique.slice(0, 4).join(' ')}` : '');
  for (const gap of unique) say(`  not carried: ${gap}`);
}

const token = MODE === 'check' ? '' : githubToken();
try {
  for (const entry of planned) {
    if (MODE === 'record') {
      if (entry.github && !token) skip(`${entry.id}.recorded`, 'no GitHub token: set GITHUB_TOKEN or run gh auth login');
      else await record(entry, token);
    }
    else await shapeCheck(entry);
  }
} catch (error) {
  check('run completes', false, error.stack);
}
const result = summary();
writeResults(`provider-answers-${MODE}`, { results, notes, gaps: gapsByCase });
process.exitCode = result.failed.length ? 1 : 0;
