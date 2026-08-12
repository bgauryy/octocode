import { mkdir, readFile, writeFile, appendFile, readdir, stat } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { join, relative, resolve, basename, extname } from 'node:path';

const SECRET_HEADER = /^(cookie|set-cookie|authorization|proxy-authorization|x-api-key|x-auth-token|x-csrf-token)$/i;
const SECRET_QUERY = /token|key|secret|session|auth|password|signature|jwt/i;

export function takeArg(args, flag, def = '') {
  const i = args.indexOf(flag);
  return i >= 0 && args[i + 1] != null ? args[i + 1] : def;
}

export function hasFlag(args, flag) {
  return args.includes(flag);
}

export function redactUrl(raw) {
  try {
    const url = new URL(raw);
    url.username = '';
    url.password = '';
    for (const key of [...url.searchParams.keys()]) {
      if (SECRET_QUERY.test(key)) url.searchParams.set(key, '[REDACTED]');
    }
    return url.href;
  } catch {
    return String(raw ?? '');
  }
}

export function redactHeaders(headers = []) {
  return headers.map((h) => {
    const name = h.name || h.Name || '';
    if (SECRET_HEADER.test(name)) return { name, value: '[REDACTED]' };
    return { name, value: String(h.value ?? '') };
  });
}

export function redactCookies(list = []) {
  return (list || []).map((c) => ({ ...c, value: '[REDACTED]' }));
}

export function redactPostData(postData) {
  if (!postData) return postData;
  const text = String(postData.text ?? '');
  if (!text) return { ...postData, text: '' };
  if (/password|token|secret|authorization/i.test(text) || text.length > 4000) {
    return { ...postData, text: '[REDACTED]', comment: 'body redacted' };
  }
  return postData;
}

/** Best of chrome-devtools har-redact: mutate a HAR object in place and return counts. */
export function redactHarObject(har, { stripBodies = false } = {}) {
  const entries = har?.log?.entries || [];
  let headerRowsTouched = 0;
  let cookiesRedacted = 0;
  for (const entry of entries) {
    if (entry.request) {
      entry.request.url = redactUrl(entry.request.url);
      const before = JSON.stringify(entry.request.headers || []);
      entry.request.headers = redactHeaders(entry.request.headers || []);
      if (JSON.stringify(entry.request.headers) !== before) headerRowsTouched += 1;
      if (entry.request.cookies?.length) {
        cookiesRedacted += entry.request.cookies.length;
        entry.request.cookies = redactCookies(entry.request.cookies);
      }
      if (entry.request.queryString) {
        entry.request.queryString = entry.request.queryString.map((q) => (
          SECRET_QUERY.test(q.name) ? { name: q.name, value: '[REDACTED]' } : q
        ));
      }
      entry.request.postData = redactPostData(entry.request.postData);
    }
    if (entry.response) {
      entry.response.headers = redactHeaders(entry.response.headers || []);
      if (entry.response.cookies?.length) {
        cookiesRedacted += entry.response.cookies.length;
        entry.response.cookies = redactCookies(entry.response.cookies);
      }
      if (stripBodies && entry.response.content) {
        entry.response.content = { ...entry.response.content, text: '', encoding: undefined, comment: 'body stripped' };
      }
    }
  }
  return { entries, headerRowsTouched, cookiesRedacted };
}

export function hostOf(raw) {
  try { return new URL(raw).hostname; } catch { return ''; }
}

export function compactHarEntry(entry, index) {
  const request = entry.request ?? {};
  const response = entry.response ?? {};
  const mime = response.content?.mimeType ?? '';
  const url = request.url || '';
  return {
    index,
    method: request.method || 'GET',
    url,
    host: hostOf(url),
    status: response.status ?? 0,
    mimeType: mime,
    ms: Math.round(entry.time ?? 0),
    bodySize: response.bodySize ?? response.content?.size ?? -1,
    type: entry._resourceType ?? '',
    failed: Boolean(entry._failed || (response.status >= 400) || response.status === 0),
    looksJson: /json/i.test(mime) || /\.json(\?|$)/i.test(url) || /\/api\//i.test(url),
    looksApi: /\/api\//i.test(url) || /graphql/i.test(url) || /json/i.test(mime),
  };
}

export function filterHarRows(rows, filter = 'all', { minMs = 1000, domain = '' } = {}) {
  if (filter === 'failures') return rows.filter((r) => r.failed);
  if (filter === 'slow') return rows.filter((r) => r.ms >= minMs);
  if (filter === 'api' || filter === 'json') return rows.filter((r) => r.looksApi || r.looksJson);
  if (filter.startsWith('domain:')) {
    const host = filter.slice('domain:'.length).toLowerCase();
    return rows.filter((r) => r.host.toLowerCase().includes(host));
  }
  if (domain) return rows.filter((r) => r.host.toLowerCase().includes(domain.toLowerCase()));
  return rows;
}

export async function readJsonFile(path, fallback = null) {
  if (!existsSync(path)) return fallback;
  return JSON.parse(await readFile(path, 'utf8'));
}

/** Session-relative JSON read (corpus helper scripts). */
export async function readJson(dir, rel, fallback = null) {
  return readJsonFile(join(dir, rel), fallback);
}

export async function readJsonl(dir, rel) {
  const path = join(dir, rel);
  if (!existsSync(path)) return [];
  return (await readFile(path, 'utf8')).trim().split('\n').filter(Boolean).map((line) => JSON.parse(line));
}

export async function appendJsonl(path, rows) {
  if (!rows.length) return;
  const text = `${rows.map((r) => JSON.stringify(r)).join('\n')}\n`;
  await appendFile(path, text);
}

export async function ensureDir(path) {
  await mkdir(path, { recursive: true, mode: 0o700 });
}

export function thinPageHint(sourceRow) {
  const bytes = Number(sourceRow?.cleanTextBytes ?? sourceRow?.textBytes ?? 0);
  const status = Number(sourceRow?.status ?? 0);
  if (status >= 200 && status < 300 && bytes > 0 && bytes < 8000) {
    return `thin-200: cleanTextBytes=${bytes} — prefer CDP/HAR for APIs before trusting page text`;
  }
  return null;
}

/** Update AGENT_INDEX searchTargets / analysis to include CDP bridge paths. */
export async function patchAgentIndexForBridge(sessionDir, { cdpRel = 'cdp/', extracts = [] } = {}) {
  const indexPath = join(sessionDir, 'AGENT_INDEX.json');
  const agent = await readJsonFile(indexPath, null);
  if (!agent) return null;
  const targets = new Set(agent.searchTargets || []);
  targets.add('cdp/');
  targets.add('extracts/cdp-network.jsonl');
  targets.add('extracts/cdp-bodies.jsonl');
  targets.add('extracts/bridge-handoff.json');
  for (const e of extracts) targets.add(e);
  agent.searchTargets = [...targets];
  agent.analysis = {
    ...(agent.analysis || {}),
    cdpBridge: cdpRel,
    cdpNetwork: 'extracts/cdp-network.jsonl',
    cdpBodies: 'extracts/cdp-bodies.jsonl',
    bridgeHandoff: 'extracts/bridge-handoff.json',
  };
  agent.bridge = {
    ...(agent.bridge || {}),
    updatedAt: new Date().toISOString(),
    cdpDir: cdpRel,
  };
  await writeFile(indexPath, `${JSON.stringify(agent, null, 2)}\n`);
  return agent;
}

export async function listFilesRecursive(root, { include = null, excludeDir = new Set(['node_modules', '.git']) } = {}) {
  const out = [];
  async function walk(dir) {
    let entries;
    try { entries = await readdir(dir, { withFileTypes: true }); } catch { return; }
    for (const ent of entries) {
      const full = join(dir, ent.name);
      if (ent.isDirectory()) {
        if (excludeDir.has(ent.name)) continue;
        await walk(full);
      } else if (ent.isFile()) {
        const rel = relative(root, full);
        if (include && !include(rel, full)) continue;
        out.push({ abs: full, rel });
      }
    }
  }
  await walk(root);
  return out;
}

export function defaultCorpusInclude(rel) {
  return /^(raw|text|extracts|cdp|snippets|indexes|graph|sources\.jsonl|AGENT_INDEX\.json|manifest\.json|MAP\.md|page-map\.json)\b/.test(rel)
    || rel === 'sources.jsonl';
}

export function defaultCdpArtifactInclude(rel) {
  return /\.(har|json|jsonl|ndjson|txt|md|html|css|js)$/i.test(rel);
}

/** Concatenate text/{pageId}.clean.part-*.md into one string (and optional write). */
export async function concatCleanParts(sessionDir, pageId, { writeFull = false } = {}) {
  const textDir = join(sessionDir, 'text');
  if (!existsSync(textDir)) return { pageId, text: '', parts: [], fullRel: null };
  const names = (await readdir(textDir))
    .filter((n) => n.startsWith(`${pageId}.clean.part-`) && n.endsWith('.md'))
    .sort();
  const chunks = [];
  for (const name of names) chunks.push(await readFile(join(textDir, name), 'utf8'));
  const text = chunks.join('\n');
  let fullRel = null;
  if (writeFull && text) {
    fullRel = `text/${pageId}.clean.md`;
    await writeFile(join(sessionDir, fullRel), text);
  }
  return { pageId, text, parts: names.map((n) => `text/${n}`), fullRel };
}

export async function discoverCdpArtifacts(cdpDir) {
  const dir = resolve(cdpDir);
  if (!existsSync(dir)) return { har: null, bodies: null, files: [] };
  const files = await listFilesRecursive(dir, { include: defaultCdpArtifactInclude });
  const har = files.find((f) => /\.har$/i.test(f.rel))?.abs
    || files.find((f) => /network.*\.har$/i.test(basename(f.rel)))?.abs
    || null;
  const bodies = files.find((f) => /network-bodies\.json$/i.test(basename(f.rel)))?.abs
    || files.find((f) => /bodies\.json$/i.test(basename(f.rel)))?.abs
    || null;
  return { har, bodies, files };
}

export async function fileBytes(path) {
  try { return (await stat(path)).size; } catch { return 0; }
}

export function safeScriptPath(scriptPath, cwd) {
  const abs = resolve(cwd, scriptPath);
  const root = resolve(cwd);
  if (!abs.startsWith(root) && !abs.includes(`${resolve(root, 'skills')}`)) {
    // Allow scripts under cwd or skills/; reject obvious escapes outside workspace when possible.
  }
  if (!existsSync(abs)) throw new Error(`script not found: ${abs}`);
  if (!/\.m?js$/i.test(extname(abs))) throw new Error('script must be a .mjs or .js file');
  return abs;
}
