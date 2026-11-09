// Recorded calls of the security-live suite: the built CLI (every response page read), provider
// requests (GitHub REST/GraphQL, OSV, the npm registry) and OSV ids with page tokens walked.
// Every call lands on the current probe, so results/security-live.json shows each one as run.
import { spawnCli } from './mcp-client.mjs';

export const GITHUB = 'https://api.github.com';
export const OSV = 'https://api.osv.dev';
/** Large enough that a row is one page; a row split anyway is reported, not joined here. */
const RESPONSE_CHARS = 1_000_000;

function providerHeaders(url, token, body) {
  const headers = { 'user-agent': 'octocode-security-live', accept: 'application/json' };
  if (url.startsWith(GITHUB)) headers.accept = 'application/vnd.github+json';
  if (url.startsWith(GITHUB) && token) headers.authorization = `Bearer ${token}`;
  if (body) headers['content-type'] = 'application/json';
  return headers;
}

function parseJson(text) {
  try { return JSON.parse(text); } catch { return undefined; }
}

function rateLimitOf(response, text) {
  const remaining = response.headers.get('x-ratelimit-remaining');
  const reset = response.headers.get('x-ratelimit-reset');
  return {
    remaining: remaining === null ? undefined : Number(remaining),
    reset: reset ? new Date(Number(reset) * 1000).toISOString() : undefined,
    limited: [403, 429].includes(response.status) && (remaining === '0' || /rate limit/i.test(text)),
  };
}

/** Why a provider read gave no truth, for a SKIP line. */
export const unreadReason = (truth, what) => `${what}: ${truth.limited ? "GitHub's rate limit" : `HTTP ${truth.status}`} (${truth.reset ? `resets ${truth.reset}` : truth.text.slice(0, 80)})`;

/** Every probe as run: its calls, provider requests and checks, for results/security-live.json. */
export const probes = [];
let current;
let settings;

/** `check`/`skip` from checks(), the contract's output validator and the per-call deadline. */
export function configureLiveCalls({ check, skip, validateOutput, timeoutMs }) {
  settings = { check, skip, validateOutput, timeoutMs };
}

/** The probe being recorded. */
export const currentProbe = () => current;

export function startProbe(mode, id, title) {
  current = { mode: mode.name, id, title, calls: [], provider: [], checks: [] };
  probes.push(current);
  const label = name => `${mode.name}.${id}.${name}`;
  const probe = {
    check: (name, ok, detail = '') => {
      current.checks.push({ name, ok: !!ok, detail: detail.slice(0, 400) });
      return settings.check(label(name), ok, detail);
    },
    skip: (name, reason) => {
      current.checks.push({ name, skipped: reason });
      settings.skip(label(name), reason);
    },
    /** Pass when `problem` is null; the detail is the problem, else `detail`. */
    expect: (name, problem, detail = '') => probe.check(name, !problem, problem ?? detail),
  };
  return probe;
}

async function page(mode, tool, request, entry) {
  const run = await spawnCli([tool, JSON.stringify(request), '--json'], { env: mode.env, timeoutMs: settings.timeoutMs });
  const json = parseJson(run.stdout);
  entry.pages.push({ exit: run.status, ms: Math.round(run.ms), bytes: run.stdout.length });
  entry.exit = run.status;
  // A call killed at the deadline while it waits for GitHub's quota reset: every later GitHub read
  // of this mode would wait the same, so the runner skips the rest of the mode with this reason.
  const waiting = run.status === null && run.stderr.match(/GitHub \w+ limit reached: waiting \d+s[^\n]*/)?.[0];
  if (waiting) mode.quotaWait = `the tool waits for GitHub's quota instead of answering (${waiting})`;
  if (!json) entry.problem = waiting ? `killed at the ${settings.timeoutMs} ms deadline: ${mode.quotaWait}` : `no JSON, exit ${run.status}: ${run.stderr.slice(0, 300)}`;
  else entry.problem ??= settings.validateOutput(tool, json);
  return json;
}

/** One tool call through the built CLI; every response page is read before it returns. */
export async function cli(mode, tool, input) {
  const first = { ...input, responseChars: RESPONSE_CHARS };
  const entry = { surface: 'cli', tool, input: first, argv: `octocode ${tool} '${JSON.stringify(first)}' --json`, pages: [], problem: null };
  current.calls.push(entry);
  const results = [];
  let json = await page(mode, tool, first, entry);
  results.push(...(json?.results ?? []));
  while (json?.responsePagination?.next) {
    json = await page(mode, tool, json.responsePagination.next.input, entry);
    results.push(...(json?.results ?? []));
  }
  const parts = results.filter(row => row.rowPart).length;
  if (parts) entry.problem ??= `${parts} row parts at responseChars ${RESPONSE_CHARS}`;
  const rows = [...new Map(results.map(row => [row.index, row])).values()].sort((a, b) => a.index - b.index);
  return { json, rows, exit: entry.exit, problem: entry.problem, data: rows[0]?.data, status: rows[0]?.status };
}

/** A provider request, recorded with its status. GitHub requests carry `token` when given. */
export async function provider(url, { method = 'GET', body, token } = {}) {
  const response = await fetch(url, { method, headers: providerHeaders(url, token, body), ...(body && { body: JSON.stringify(body) }) });
  const text = await response.text();
  const limit = rateLimitOf(response, text);
  current.provider.push({ method, url, status: response.status, remaining: limit.remaining });
  return { status: response.status, json: parseJson(text), text, ...limit };
}

const osvCache = new Map();
/** Every OSV id for each query, following each query's page token; cached for the run. */
export async function osvIds(queries) {
  const keys = queries.map(query => JSON.stringify(query));
  let ask = [...new Set(keys.filter(key => !osvCache.has(key)))].map(key => ({ key, query: JSON.parse(key), ids: [] }));
  const pending = ask;
  while (ask.length) {
    const body = { queries: ask.map(item => (item.token ? { ...item.query, page_token: item.token } : item.query)) };
    const answer = await provider(`${OSV}/v1/querybatch`, { method: 'POST', body });
    if (answer.status !== 200) throw new Error(`OSV querybatch HTTP ${answer.status}: ${answer.text.slice(0, 120)}`);
    ask.forEach((item, i) => {
      item.ids.push(...(answer.json.results[i].vulns ?? []).map(vuln => vuln.id));
      item.token = answer.json.results[i].next_page_token;
    });
    ask = ask.filter(item => item.token);
  }
  for (const item of pending) osvCache.set(item.key, [...new Set(item.ids)]);
  return keys.map(key => osvCache.get(key));
}

/** The npm registry's packument (`dist-tags`, `versions`, `time`). */
export const registry = async name => (await provider(`https://registry.npmjs.org/${name}`)).json;
