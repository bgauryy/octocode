// Loopback mock for the external providers the tool-health suite exercises:
// GitHub REST (GITHUB_API_URL), an npm registry (artifactSearch `registryUrl`)
// and the jev classification provider (OCTOCODE_CLASSIFICATION_API_HOST).
// Responses come from one small in-memory model (mock-model.mjs: repositories
// whose files are the tool-health fixture), so every endpoint answers
// consistently and filters/pages generically; nothing keys on a test input.
import http from 'node:http';
import path from 'node:path';
import { isMainModule } from '../../../../../../skills/common/scripts/lib/entrypoint.mjs';
import { makeGitRepo, serveGit } from './mock-git.mjs';
import { buildModel } from './mock-model.mjs';
import { route } from './mock-routes.mjs';

/**
 * GitHub's rate-limit resources and their per-window quotas: REST search is 30
 * a minute and code search 10 a minute (docs.github.com/en/rest/search/search#rate-limit);
 * core and GraphQL are 5,000 an hour.
 */
const RATE_LIMIT_RESOURCES = {
  code_search: { limit: 10, windowSeconds: 60 },
  search: { limit: 30, windowSeconds: 60 },
  graphql: { limit: 5000, windowSeconds: 3600 },
  core: { limit: 5000, windowSeconds: 3600 },
};

/** The rate-limit resource a (normalized) path draws from. */
const resourceOf = p => (p === '/search/code' ? 'code_search' : p.startsWith('/search/') ? 'search' : /^\/(?:api\/)?graphql$/.test(p) ? 'graphql' : 'core');

/**
 * Quota per credential and resource, as GitHub meters it: a request counts in
 * the window it opens, and its answer reports the quota left and the window's
 * reset. The mock reports the quota; it never refuses a request over it.
 */
const rateLimiter = () => {
  const windows = new Map();
  return (resource, credential, now = Date.now()) => {
    const { limit, windowSeconds } = RATE_LIMIT_RESOURCES[resource];
    const key = `${resource}\0${credential ?? ''}`;
    let window = windows.get(key);
    if (!window || window.reset * 1000 <= now) window = { used: 0, reset: Math.floor(now / 1000) + windowSeconds };
    window.used += 1;
    windows.set(key, window);
    return { 'x-ratelimit-limit': String(limit), 'x-ratelimit-remaining': String(Math.max(limit - window.used, 0)), 'x-ratelimit-reset': String(window.reset), 'x-ratelimit-used': String(window.used), 'x-ratelimit-resource': resource };
  };
};

/** A JSON (or plain-text) responder with GitHub's rate-limit headers. */
const sender = (res, rateLimit) => (status, payload, headers = {}) => {
  const text = typeof payload === 'string' ? payload : JSON.stringify(payload);
  res.writeHead(status, { 'content-type': typeof payload === 'string' ? 'text/plain' : 'application/json; charset=utf-8', ...rateLimit, ...headers });
  res.end(text);
};

/**
 * Start the mock on an ephemeral loopback port. GitHub REST answers at the
 * root and under `/api/v3` (a GitHub Enterprise base, which ghCloneRepo maps to
 * `<host>/<owner>/<repo>.git`). `requests` records every request; `unhandled`
 * the ones no route answered (a suite failure detail), and `unhandledFrom` the
 * same by the credential that sent them. With `tmpDir`, git smart HTTP serves
 * `octo/demo.git` behind Basic auth: `gitAuth` records each credential check as
 * `{ user, tokenMatches }` (never the secret itself). `token`: the credential
 * git accepts, or a list of them (one per caller).
 */
export async function startMock({ fixtureDir, tmpDir, token, configureModel, classificationResponder } = {}) {
  const model = buildModel(fixtureDir);
  configureModel?.(model);
  const requests = [];
  const requestsFrom = [];
  const classification = { requests: [], responder: classificationResponder, responders: new Map() };
  const unhandled = [];
  const unhandledFrom = [];
  const miss = (req, what) => {
    unhandled.push(what);
    unhandledFrom.push({ request: what, token: /^(?:token|bearer)\s+(.+)$/i.exec(req.headers.authorization ?? '')?.[1] ?? null });
  };
  const meter = rateLimiter();
  const repoGit = tmpDir ? makeGitRepo(model, tmpDir) : { reason: 'no tmpDir' };
  const git = { root: repoGit.root, tokens: new Set([token].flat().filter(Boolean)), gitAuth: [] };
  let base = '';
  const handle = async (req, res, raw) => {
    const url = new URL(req.url, base);
    requests.push(`${req.method} ${url.pathname}${url.search}`);
    requestsFrom.push({ request: requests.at(-1), token: /^(?:token|bearer)\s+(.+)$/i.exec(req.headers.authorization ?? '')?.[1] ?? null });
    if (git.root && /^\/[^/]+\/[^/]+\.git(\/|$)/.test(url.pathname)) { serveGit({ req, url, raw }, res, git); return; }
    const p = (decodeURIComponent(url.pathname).replace(/\/+$/, '') || '/').replace(/^\/api\/v3(?=\/|$)/, '') || '/';
    const send = sender(res, meter(resourceOf(p), requestsFrom.at(-1).token));
    try {
      const handled = await route({ method: req.method, url, p, body: raw.toString('utf8'), headers: req.headers, send, model, base, classification });
      if (!handled) { miss(req, `${req.method} ${url.pathname}${url.search}`); send(404, { message: 'Not Found' }); }
    } catch (error) {
      miss(req, `${req.method} ${url.pathname}: ${error.message}`);
      send(500, { message: error.message });
    }
  };
  const server = http.createServer((req, res) => {
    const chunks = [];
    req.on('data', chunk => { chunks.push(chunk); });
    req.on('end', () => handle(req, res, Buffer.concat(chunks)));
  });
  await new Promise((resolve, reject) => { server.once('error', reject); server.listen(0, '127.0.0.1', resolve); });
  base = `http://127.0.0.1:${server.address().port}`;
  return {
    url: base, model, requests, requestsFrom, classificationRequests: classification.requests, classificationResponders: classification.responders, unhandled, unhandledFrom, gitAuth: git.gitAuth,
    git: !!git.root, gitUnavailable: repoGit.reason,
    close: () => new Promise(resolve => { server.closeAllConnections?.(); server.close(() => resolve()); }),
  };
}

// `node mock-server.mjs <fixtureDir>`: run standalone for debugging.
// argv[1] keeps a symlinked path while import.meta.url is the real path, so compare real paths.
if (isMainModule(import.meta.url)) {
  const mock = await startMock({ fixtureDir: path.resolve(process.argv[2] ?? '.') });
  console.log(mock.url);
  let shown = 0;
  setInterval(() => {
    while (shown < mock.requests.length) console.error(new Date().toISOString(), JSON.stringify(mock.requests[shown++]).slice(0, 240));
    while (mock.unhandled.length) console.error('UNHANDLED', mock.unhandled.shift());
  }, 100);
}
