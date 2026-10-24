import { mkdir, readFile, writeFile, readdir, stat } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { join, relative, resolve, basename, extname } from 'node:path';
import { fail } from './fail.mjs';

const SECRET_HEADER = /^(cookie|set-cookie|authorization|proxy-authorization|x-api-key|x-auth-token|x-access-token|x-csrf-token|x-xsrf-token|x-amz-security-token|x-goog-api-key|api-key)$/i;
const SECRET_QUERY = /token|key|secret|session|auth|password|signature|jwt/i;
// A field whose value is a credential: matched on the key's ending so `max_tokens` or `tokenType` stay readable.
const SECRET_FIELD = /(?:token|secret|passw(?:or)?d|passcode|pwd|api[_-]?key|authorization|credentials?|signature|jwt|private[_-]?key|session[_-]?id|cookie)$|^(?:auth|sid|otp|pin|pass)$/i;
// An input whose label names a credential; its typed value is redacted in DOM captures.
const SECRET_LABEL = /passw|passcode|\bpin\b|\botp\b|one[- ]time|token|secret|api[ _-]?key|credential|\bcvc\b|\bcvv\b/i;
const TEXTUAL_MIME = /json|text|xml|javascript|x-www-form-urlencoded|graphql/i;
const REDACTED = '[REDACTED]';
const TOKEN_SHAPES = [
  /-----BEGIN [A-Z ]*PRIVATE KEY-----[\s\S]*?-----END [A-Z ]*PRIVATE KEY-----/g,
  /\beyJ[A-Za-z0-9_-]{5,}\.[A-Za-z0-9_-]{5,}\.[A-Za-z0-9_-]{5,}/g,
  /\b(?:gh[pousr]_[A-Za-z0-9]{30,}|github_pat_[A-Za-z0-9_]{30,})\b/g,
  /\bAKIA[0-9A-Z]{16}\b/g,
  /\bxox[abprs]-[A-Za-z0-9-]{10,}/g,
  /\bsk-[A-Za-z0-9_-]{20,}/g,
  /\bAIza[0-9A-Za-z_-]{35}\b/g,
];

const secretKey = (key) => SECRET_FIELD.test(String(key).replace(/[[\]]/g, ''));

/**
 * Mask credential values inside captured text (JSON, form bodies, header lines, known token shapes).
 * Only the secret values change; the surrounding evidence stays readable.
 */
export function redactSecretsInText(input) {
  if (typeof input !== 'string' || !input) return input;
  let text = input;
  for (const shape of TOKEN_SHAPES) text = text.replace(shape, REDACTED);
  text = text.replace(/\b(Bearer|Basic|Token)\s+[A-Za-z0-9\-._~+/=]{6,}/g, `$1 ${REDACTED}`);
  // JSON pairs, then the same pairs inside an escaped JSON string.
  text = text.replace(/("((?:[^"\\]|\\.)*)"\s*:\s*)("(?:[^"\\]|\\.)*"|-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?|true|false)/g,
    (match, prefix, key) => (secretKey(key) ? `${prefix}"${REDACTED}"` : match));
  text = text.replace(/(\\"([^"\\]*)\\"\s*:\s*)(\\"(?:[^"\\]|\\(?!"))*\\"|-?\d+(?:\.\d+)?)/g,
    (match, prefix, key) => (secretKey(key) ? `${prefix}\\"${REDACTED}\\"` : match));
  // Form and query pairs.
  text = text.replace(/(^|[?&;\s])([A-Za-z0-9_.\-[\]]+)=([^&\s;#"'<>]+)/gm,
    (match, lead, key) => (secretKey(key) ? `${lead}${key}=${REDACTED}` : match));
  // Header-style lines.
  text = text.replace(/^([A-Za-z][\w-]*)(\s*:\s*)(\S.*)$/gm,
    (match, key, sep) => (SECRET_HEADER.test(key) || secretKey(key) ? `${key}${sep}${REDACTED}` : match));
  return text;
}

/** Deep-redact parsed JSON: secret keys lose their values, labelled credential inputs lose typed text. */
export function redactSecretsInJson(value) {
  if (typeof value === 'string') return redactSecretsInText(value);
  if (Array.isArray(value)) return value.map(redactSecretsInJson);
  if (!value || typeof value !== 'object') return value;
  const labelled = Object.entries(value).some(([k, v]) => (
    /^(name|label|type|autocomplete|accessibleNameGuess|placeholder|id)$/i.test(k) && typeof v === 'string' && SECRET_LABEL.test(v)
  ));
  const out = {};
  for (const [k, v] of Object.entries(value)) {
    if (secretKey(k) && v != null && v !== '') out[k] = REDACTED;
    else if (labelled && /^(value|text|defaultValue)$/i.test(k) && typeof v === 'string' && v) out[k] = REDACTED;
    else out[k] = redactSecretsInJson(v);
  }
  return out;
}

/** Redact a HAR content object's text, decoding textual base64 bodies first; binary bodies pass through. */
export function redactHarContent(content) {
  if (!content || typeof content.text !== 'string' || !content.text) return content;
  if (content.encoding === 'base64') {
    if (!TEXTUAL_MIME.test(content.mimeType || '')) return content;
    const decoded = Buffer.from(content.text, 'base64').toString('utf8');
    const redacted = redactSecretsInText(decoded);
    return redacted === decoded ? content : { ...content, text: Buffer.from(redacted, 'utf8').toString('base64') };
  }
  return { ...content, text: redactSecretsInText(content.text) };
}

export function takeArg(args, flag, def = '') {
  const i = args.indexOf(flag);
  if (i < 0) return def;
  const value = args[i + 1];
  // A following flag is not a value: `--session-dir --query x` must fail, not read "--query" as a path.
  if (value == null || value.startsWith('--')) fail('invalidArguments', `missing value for ${flag}`, 2);
  return value;
}

export function hasFlag(args, flag) {
  return args.includes(flag);
}

export function redactUrl(raw) {
  try {
    const url = new URL(raw);
    url.username = '';
    url.password = '';
    for (const key of Array.from(url.searchParams.keys())) {
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
  const out = { ...postData };
  if (typeof out.text === 'string') out.text = redactSecretsInText(out.text);
  if (Array.isArray(out.params)) {
    out.params = out.params.map((p) => (secretKey(p?.name ?? '') ? { ...p, value: REDACTED } : p));
  }
  return out;
}

// Redacts one request in place; returns whether its headers changed and how many cookies it carried.
function redactHarRequest(request) {
  request.url = redactUrl(request.url);
  const before = JSON.stringify(request.headers || []);
  request.headers = redactHeaders(request.headers || []);
  const touched = JSON.stringify(request.headers) !== before;
  let cookies = 0;
  if (request.cookies?.length) {
    cookies = request.cookies.length;
    request.cookies = redactCookies(request.cookies);
  }
  if (request.queryString) {
    request.queryString = request.queryString.map((q) => (
      SECRET_QUERY.test(q.name) ? { name: q.name, value: '[REDACTED]' } : q
    ));
  }
  request.postData = redactPostData(request.postData);
  return { touched, cookies };
}

// Redacts one response in place; returns how many cookies it carried.
function redactHarResponse(response, stripBodies) {
  response.headers = redactHeaders(response.headers || []);
  let cookies = 0;
  if (response.cookies?.length) {
    cookies = response.cookies.length;
    response.cookies = redactCookies(response.cookies);
  }
  if (stripBodies && response.content) {
    response.content = { ...response.content, text: '', encoding: undefined, comment: 'body stripped' };
  } else if (response.content) {
    response.content = redactHarContent(response.content);
  }
  return cookies;
}

/** Best of chrome-devtools har-redact: mutate a HAR object in place and return counts. */
export function redactHarObject(har, { stripBodies = false } = {}) {
  const entries = har?.log?.entries || [];
  let headerRowsTouched = 0;
  let cookiesRedacted = 0;
  for (const entry of entries) {
    if (entry.request) {
      const { touched, cookies } = redactHarRequest(entry.request);
      if (touched) headerRowsTouched += 1;
      cookiesRedacted += cookies;
    }
    if (entry.response) cookiesRedacted += redactHarResponse(entry.response, stripBodies);
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
    bodySize: bodySizeOf(response),
    type: entry._resourceType ?? '',
    failed: entryFailed(entry, response),
    looksJson: /json/i.test(mime) || /\.json(\?|$)/i.test(url) || /\/api\//i.test(url),
    looksApi: /\/api\//i.test(url) || /graphql/i.test(url) || /json/i.test(mime),
  };
}

const bodySizeOf = (response) => response.bodySize ?? response.content?.size ?? -1;
const entryFailed = (entry, response) => Boolean(entry._failed || (response.status >= 400) || response.status === 0);

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

/** Stop a corpus query on a folder that is not a scrape session, instead of answering "0 matches". */
export function requireSession(dir) {
  const markers = ['AGENT_INDEX.json', 'sources.jsonl', 'page-map.json', 'manifest.json', 'graph', 'extracts'];
  if (markers.some((rel) => existsSync(join(dir, rel)))) return;
  fail('sessionNotFound', `not a scrape session (none of ${markers.join(', ')}): ${dir}`, 1, { sessionDir: dir });
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

/** Rewrite a JSONL file: drop rows that `isStale` matches, then append `rows`. */
export async function replaceJsonl(path, rows, isStale) {
  const kept = existsSync(path)
    ? (await readFile(path, 'utf8')).split('\n').filter(Boolean).filter((line) => !isStale(JSON.parse(line)))
    : [];
  const all = [...kept, ...rows.map((r) => JSON.stringify(r))];
  if (!all.length) return;
  await writeFile(path, `${all.join('\n')}\n`);
}

export async function ensureDir(path) {
  await mkdir(path, { recursive: true, mode: 0o700 });
}

export function thinPageHint(sourceRow) {
  const bytes = Number(sourceRow?.cleanTextBytes ?? sourceRow?.textBytes ?? 0);
  const status = Number(sourceRow?.status ?? 0);
  if (sourceRow?.browserRecommended) {
    return sourceRow.browserReason || `browser recommended for ${sourceRow.pageId || sourceRow.url}`;
  }
  // Compatibility for corpora written before browserRecommended existed. A
  // few kilobytes is normal for a focused docs page; only near-empty 2xx text
  // is a useful legacy signal by itself.
  if (status >= 200 && status < 300 && bytes >= 0 && bytes < 300) {
    return `thin-200: cleanTextBytes=${bytes} — inspect the retained HTML before using Chrome`;
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
    ...agent.analysis,
    cdpBridge: cdpRel,
    cdpNetwork: 'extracts/cdp-network.jsonl',
    cdpBodies: 'extracts/cdp-bodies.jsonl',
    bridgeHandoff: 'extracts/bridge-handoff.json',
  };
  agent.bridge = {
    ...agent.bridge,
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
  return /^(?:(?:raw|text|extracts|cdp|snippets|indexes|graph|reports)\/|(?:sources\.jsonl|AGENT_INDEX\.json|manifest\.json|MAP\.md|page-map\.json)$)/.test(rel);
}

export function defaultCdpArtifactInclude(rel) {
  return /\.(har|json|jsonl|ndjson|txt|md|html|css|js)$/i.test(rel);
}

const cleanPartNumber = (name) => Number((name.match(/\.clean\.part-(\d+)\.md$/) || [])[1] || 0);

/** Concatenate text/{pageId}.clean.part-*.md into one string (and optional write). */
export async function concatCleanParts(sessionDir, pageId, { writeFull = false } = {}) {
  const textDir = join(sessionDir, 'text');
  if (!existsSync(textDir)) return { pageId, text: '', parts: [], fullRel: null };
  const names = (await readdir(textDir))
    .filter((n) => n.startsWith(`${pageId}.clean.part-`) && n.endsWith('.md'))
    .toSorted((a, b) => cleanPartNumber(a) - cleanPartNumber(b));
  const chunks = [];
  for (const name of names) chunks.push(await readFile(join(textDir, name), 'utf8'));
  // Parts are byte-exact splits of one text, so they join with nothing between them.
  const text = chunks.join('');
  let fullRel = null;
  if (writeFull && text) {
    fullRel = `text/${pageId}.clean.md`;
    await writeFile(join(sessionDir, fullRel), text);
  }
  return { pageId, text, parts: names.map((n) => `text/${n}`), fullRel };
}

export async function discoverCdpArtifacts(cdpDir) {
  const dir = resolve(cdpDir);
  if (!existsSync(dir)) return { har: null, bodies: null, captures: [], files: [] };
  const files = await listFilesRecursive(dir, { include: defaultCdpArtifactInclude });
  const har = files.find((f) => /\.har$/i.test(f.rel))?.abs
    || files.find((f) => /network.*\.har$/i.test(basename(f.rel)))?.abs
    || null;
  const bodies = files.find((f) => /network-bodies\.json$/i.test(basename(f.rel)))?.abs
    || files.find((f) => /bodies\.json$/i.test(basename(f.rel)))?.abs
    || null;
  const captures = files.filter((f) => /^(?:page-snapshot|dom-check)\.json$/i.test(basename(f.rel))).map((f) => f.abs);
  return { har, bodies, captures, files };
}

export async function fileBytes(path) {
  try { return (await stat(path)).size; } catch { return 0; }
}

export function safeScriptPath(scriptPath, cwd) {
  const abs = resolve(cwd, scriptPath);
  if (!existsSync(abs)) throw new Error(`script not found: ${abs}`);
  if (!/\.m?js$/i.test(extname(abs))) throw new Error('script must be a .mjs or .js file');
  return abs;
}
