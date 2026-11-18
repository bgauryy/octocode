// Records real provider answers into test-utils/provider-answers, which the native mocks replay
// (crates/tool-packages/src/artifact/test_http/recorded.rs, crates/runtime/tests/support/recorded.rs).
//
//   node record.mjs [id,...]       re-record live, read-only and public: `dev.mjs answers:record`
//
// Each case is one tool call through the built CLI (recorded-plan.mjs); a TLS proxy forwards its
// provider requests and keeps each answer. A recording is refused, and the old file kept, when an
// answer holds the token or a private repository, when trimming loses a field path, or when the
// trimmed recording makes the tool ask anything else. A case's `unread` list survives a refresh.
// OCTOCODE_NATIVE_BIN with NODE_ENV=test or development runs a chosen binary.
import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import fs from 'node:fs';
import net from 'node:net';
import os from 'node:os';
import path from 'node:path';
import { githubToken } from '../lib/github-token.mjs';
import { CLI } from './paths.mjs';
import { startRegistryProxy } from './mock-registry-proxy.mjs';
import { REGISTRY_HOSTS } from './mock-routes-registry.mjs';
import { casePath, hasCase, hostsOf, readCase, replayer, requestKey, served, writeCase } from './recorded-answers.mjs';
import { shapeDrift, shapeOf, trimmed } from './recorded-shape.mjs';
import { EXTRA_HOSTS, PLAN } from './recorded-plan.mjs';

const named = process.argv.slice(2).filter(arg => !arg.startsWith('--')).flatMap(arg => arg.split(',')).filter(Boolean);
const planned = named.length ? PLAN.filter(entry => named.some(id => entry.id === id || entry.id.startsWith(`${id}/`))) : PLAN;
if (named.length && !planned.length) throw new Error(`no recording named ${named.join(', ')}`);

const KEPT_HEADERS = new Set(['content-type', 'link', 'etag', 'cache-control', 'retry-after']);
const DROPPED_REQUEST_HEADERS = new Set(['host', 'connection', 'content-length', 'accept-encoding', 'proxy-connection']);
const DEAD_PROXY = 'http://127.0.0.1:9';
const ISOLATED_ENV = /^(GITHUB_|GH_|OCTOCODE_|NPM_CONFIG_|npm_config_|GIT_)/;
const ISOLATED_NAMES = new Set(['ALLOWED_PATHS', 'WORKSPACE_ROOT', 'TOOLS_TO_RUN', 'DISABLE_TOOLS']);

let passed = 0;
const failed = [];
const say = line => process.stdout.write(`${line}\n`);
const check = (name, ok, detail = '') => {
  say(`${ok ? 'PASS' : 'FAIL'} ${name}${detail ? ` — ${String(detail).slice(0, 160)}` : ''}`);
  if (ok) passed += 1; else failed.push(name);
  return ok;
};

/** The caller's environment without credentials or Octocode settings, in a fresh home. */
function isolatedEnv(home, tmp) {
  const inherited = Object.fromEntries(Object.entries(process.env).filter(([name]) => !ISOLATED_ENV.test(name) && !ISOLATED_NAMES.has(name)));
  const nativeBin = process.env.OCTOCODE_NATIVE_BIN?.trim();
  return {
    ...inherited,
    NODE_ENV: process.env.NODE_ENV === 'development' ? 'development' : 'test', ...(nativeBin && { OCTOCODE_NATIVE_BIN: nativeBin }),
    OCTOCODE_SECURITY_CHECKS: 'true',
    HOME: home, USERPROFILE: home, XDG_CONFIG_HOME: path.join(home, '.config'), GH_CONFIG_DIR: path.join(home, '.gh'),
    TMPDIR: tmp, TMP: tmp, TEMP: tmp, GIT_CONFIG_NOSYSTEM: '1', GIT_TERMINAL_PROMPT: '0',
    NO_PROXY: '127.0.0.1,localhost,::1', no_proxy: '127.0.0.1,localhost,::1',
    HTTP_PROXY: DEAD_PROXY, http_proxy: DEAD_PROXY, ALL_PROXY: DEAD_PROXY, all_proxy: DEAD_PROXY,
    OCTOCODE_HOME: path.join(home, '.octocode'), OCTOCODE_LSP_AUTO_INSTALL: 'off', OCTOCODE_LSP_PREWARM: 'off',
  };
}

function runCli(argv, { cwd, env }) {
  return new Promise((resolve, reject) => {
    const child = spawn(process.execPath, [CLI, ...argv], { cwd, env, stdio: ['ignore', 'pipe', 'pipe'] });
    let stdout = '', stderr = '';
    child.stdout.setEncoding('utf8').on('data', chunk => { stdout += chunk; });
    child.stderr.setEncoding('utf8').on('data', chunk => { stderr += chunk; });
    child.on('error', reject);
    child.on('close', status => resolve({ status, stdout, stderr }));
  });
}

/** The warm servers the CLI started with `TMPDIR=tmp` (sockets in `tmp/octocode-<uid>`): ask each to stop, then wait until none answers. */
async function stopBrokers(tmp) {
  const sockets = () => {
    try {
      return fs.readdirSync(tmp, { withFileTypes: true })
        .filter(entry => entry.isDirectory() && entry.name.startsWith('octocode-'))
        .flatMap(entry => fs.readdirSync(path.join(tmp, entry.name)).map(name => path.join(tmp, entry.name, name)))
        .filter(file => fs.statSync(file, { throwIfNoEntry: false })?.isSocket());
    } catch { return []; }
  };
  const reach = (socketPath, stop) => new Promise(resolve => {
    const connection = net.createConnection(socketPath);
    const deadline = setTimeout(() => { connection.destroy(); resolve(false); }, 1000);
    connection.on('connect', () => { clearTimeout(deadline); if (stop) connection.end('{"stop":true}\n'); else connection.destroy(); resolve(true); });
    connection.on('error', () => { clearTimeout(deadline); resolve(false); });
  });
  await Promise.all(sockets().map(socketPath => reach(socketPath, true)));
  for (let wait = 0; wait < 200; wait += 1) {
    if (!(await Promise.all(sockets().map(socketPath => reach(socketPath, false)))).includes(true)) return;
    await new Promise(resolve => setTimeout(resolve, 50));
  }
}

/** One tool call, its whole response, with `answer` standing in for the providers. */
async function runCall(entry, { hosts, answer, token }) {
  const tmp = fs.mkdtempSync(path.join(process.platform === 'darwin' ? '/tmp' : os.tmpdir(), 'octocode-answers-'));
  const home = path.join(tmp, 'home');
  fs.mkdirSync(home, { recursive: true });
  const proxy = await startRegistryProxy(tmp, { hosts, answer });
  try {
    const env = { ...isolatedEnv(home, tmp), GITHUB_API_URL: 'https://api.github.com', HTTPS_PROXY: proxy.proxyUrl, https_proxy: proxy.proxyUrl, SSL_CERT_FILE: proxy.caFile, SSL_CERT_DIR: '', GITHUB_TOKEN: token };
    const outputs = [];
    let input = { queries: [entry.query], responseChars: 1_000_000 };
    while (input) {
      const run = await runCli([entry.tool, JSON.stringify(input), '--json'], { cwd: tmp, env });
      let output;
      try { output = JSON.parse(run.stdout); } catch { return { outputs, failure: `not JSON, exit ${run.status}: ${(run.stdout + run.stderr).slice(0, 200)}` }; }
      outputs.push(output);
      // A split response finishes; the provider's next page is another request, not this case.
      input = output.responsePagination?.next?.input;
    }
    return { outputs, denied: proxy.denied };
  } finally {
    await proxy.close();
    await stopBrokers(tmp);
    fs.rmSync(tmp, { recursive: true, force: true });
  }
}

/** The provider's next page, from a Link header. */
const nextFrom = link => /<([^>]+)>;\s*rel="next"/.exec(link ?? '')?.[1];
const EMAIL = /[A-Za-z0-9._%+-]+@[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)+/g;
// `scheme://user:password@host`: a password in a URL's user information.
const URL_PASSWORD = /(\b[a-z][a-z0-9+.-]*:\/\/[^\s:@/"'`\\]+):[^\s@/"'`\\]+@/gi;
/**
 * Public, but not stored as written: a person's address goes, and a password in a
 * URL's user information goes (an advisory's own example of one), so the store never
 * holds what the engine's secret detector redacts. The shape stays.
 */
const scrubbed = text => text.replace(EMAIL, 'person@example.test').replace(URL_PASSWORD, '$1@');

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
  try { return { ...common, body: JSON.parse(scrubbed(text)) }; } catch { return { ...common, body: scrubbed(text) }; }
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

/**
 * The stored body of `exchanges[index]`: `trimmed` keeps what any other request
 * of the call carries, since any of them may be built from this answer. Every
 * other request, not only the ones stored after it: a request asked again keeps
 * its first place in the store.
 */
const shapeKept = (entry, exchanges, index, pinned) => {
  const fields = exchanges[index].body;
  const requests = exchanges.filter((exchange, other) => other !== index && !exchange.alternate).map(exchange => exchange.request);
  return fields && typeof fields === 'object' ? trimmed(fields, { maps: entry.maps, pinned, requests, own: exchanges[index].request }) : fields;
};

const sensitive = (exchange, token) => {
  const text = JSON.stringify(exchange);
  if (token && text.includes(token)) return 'holds the token it was recorded with';
  return /"private":\s*true/.test(text) ? 'holds a private repository' : null;
};

async function record(entry, token) {
  const run = await liveRun(entry, token);
  const unsafe = run.exchanges.map(exchange => sensitive(exchange, token)).find(Boolean);
  const refused = run.exchanges.filter(exchange => exchange.status >= 500 || exchange.status === 429 || exchange.status === 401 || exchange.status === 403);
  const pinned = [entry.query.version].filter(Boolean);
  const lost = run.exchanges.flatMap((exchange, index) => {
    if (!exchange.body || typeof exchange.body !== 'object') return [];
    const kept = shapeDrift(shapeOf(exchange.body, entry.maps), shapeOf(shapeKept(entry, run.exchanges, index, pinned), entry.maps));
    return kept.failures.map(text => `trimming lost ${text}: ${exchange.request.url}`);
  });
  if (lost.length) { check(`${entry.id}.recorded`, false, lost[0]); return; }
  if (!check(`${entry.id}.recorded`, !unsafe && !run.failure && !run.failures.length && !refused.length && run.exchanges.length > 0, unsafe ?? run.failure ?? run.failures[0] ?? (refused[0] && `${refused[0].status} ${refused[0].request.url}`) ?? '')) return;
  const before = hasCase(entry.id) ? fs.readFileSync(casePath(entry.id), 'utf8') : null;
  const annotations = new Map((before ? JSON.parse(before).exchanges : []).map(exchange => [requestKey(exchange.request), {
    ...(exchange.unread && { unread: exchange.unread }), ...(exchange.unreadReasons && { unreadReasons: exchange.unreadReasons }),
  }]));
  writeCase(entry.id, {
    case: entry.id, tool: entry.tool, query: entry.query, recordedAt: new Date().toISOString().slice(0, 10),
    unread: hasCase(entry.id) ? readCase(entry.id).unread : [],
    exchanges: run.exchanges.map((exchange, index) => ({ ...exchange, ...annotations.get(requestKey(exchange.request)), ...(exchange.body !== undefined && { body: shapeKept(entry, run.exchanges, index, pinned) }) })),
  });
  await replaysAsAsked(entry, run, before);
}

/** A trimmed answer that changes what the tool asks next has lost more than text: keep the old file. */
async function replaysAsAsked(entry, run, before) {
  const recording = readCase(entry.id);
  const { answer, asked, unrecorded } = replayer(recording);
  await runCall(entry, { hosts: [...new Set([...REGISTRY_HOSTS, ...EXTRA_HOSTS, ...hostsOf(recording)])], answer, token: 'recorded-answers-fake-token' });
  const live = run.exchanges.filter(exchange => !exchange.alternate).map(exchange => requestKey(exchange.request));
  const differs = [...live.filter(key => !asked.has(key)), ...unrecorded];
  if (check(`${entry.id}.replays`, !differs.length, differs[0]?.slice(0, 200) ?? '')) {
    // Growth shows in review: what the store holds for this case now, beside what it held.
    say(`NOTE ${entry.id}: stored ${fs.statSync(casePath(entry.id)).size} bytes (was ${before === null ? 'none' : Buffer.byteLength(before)})`);
    return;
  }
  if (before === null) fs.rmSync(casePath(entry.id));
  else fs.writeFileSync(casePath(entry.id), before);
}

const token = githubToken();
try {
  for (const entry of planned) {
    if (entry.github && !token) say(`SKIP ${entry.id}.recorded — no GitHub token: set GH_TOKEN or GITHUB_TOKEN, or run gh auth login`);
    else await record(entry, token);
  }
} catch (error) {
  check('run completes', false, error.stack);
}
say(`\n${passed}/${passed + failed.length} recorded${failed.length ? `; failed: ${failed.join(', ')}` : ''}`);
process.exitCode = failed.length ? 1 : 0;
