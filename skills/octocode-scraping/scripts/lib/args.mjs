import { createHash } from 'node:crypto';
import { isAbsolute, relative, resolve, sep, join } from 'node:path';
import { resolveProvider } from './providers.mjs';

/**
 * Anchor output outside any `.octocode` tree: a shell whose cwd drifted into an
 * existing session must not nest a new session inside it. Returns the directory
 * just above the FIRST `.octocode` segment of cwd, or cwd itself.
 */
export function workspaceRootFor(cwd) {
  const parts = resolve(cwd).split(sep);
  const octocodeAt = parts.indexOf('.octocode');
  if (octocodeAt <= 0) return resolve(cwd);
  return parts.slice(0, octocodeAt).join(sep) || sep;
}

export const MODE_ENDPOINT = { html: 'general', markdown: 'markdown', extended: 'extended', extract: 'extract' };

function usage(exitCode = 2) {
    console.error(`Usage: fetch.mjs --url <url> [--provider scrapingant|direct|cdp] [--mode html|markdown|extended|extract] [--extract-properties <text>] [--crawl --max-pages <n> [--sitemap] [--span-hosts] [--delay-ms <n>]] [--allow-private-host] [--session <id> [--append]] [--out <dir>] [--browser] [--wait-for <selector>] [--proxy-type <type>] [--proxy-country <cc>] [--block-resource <type>]... [--no-raw] [--max-raw-bytes <n>] [--max-text-bytes <n>] [--chunk-bytes <n>] [--param k=v]... [--cdp-port <n>] [--cdp-wait-ms <n>] [--wait-text <text>] [--cdp-stealth|--no-cdp-stealth]\nLinks are always extracted. A crawl stays on the --url host unless --span-hosts is passed. Private, local, and link-local hosts are refused, including crawled links, redirects, and browser requests; --allow-private-host exempts only the --url origin (a local dev server). Tests only: --mock-status <n> --mock-body-file <file> [--mock-content-type <type>] [--mock-credit-cost <n>]\nDefault output: .octocode/octocode-scraping/tmp/<sessionId>\n`);
    process.exit(exitCode);
}

export function createArgParser(args) {
  const take = (flag) => {
    const i = args.indexOf(flag);
    if (i === -1) return undefined;
    const v = args[i + 1];
    if (!v || v.startsWith('--')) usage();
    return v;
  };
  const has = (flag) => args.includes(flag);
  const allRepeated = (flag) => {
    const out = [];
    for (let i = 0; i < args.length; i += 1) if (args[i] === flag) {
      const v = args[i + 1];
      if (!v || v.startsWith('--')) usage();
      out.push(v); i += 1;
    }
    return out;
  };
  return { usage, take, has, allRepeated };
}

export function parsePassParams(allRepeated) {
  return allRepeated('--param').map((raw) => {
    if (!raw.includes('=')) throw new Error('--param must be key=value');
    const [key, ...rest] = raw.split('=');
    if (/api[-_]?key|token|cookie|authorization|secret/i.test(key)) throw new Error(`Refusing secret-like --param key: ${key}`);
    return [key, rest.join('=')];
  });
}

export function safeSessionId(input, targetUrl) {
  if (input) {
    const cleaned = input.replace(/[^A-Za-z0-9._-]/g, '-').slice(0, 120);
    // `.` and `..` would resolve to the scrape folder or its parent instead of a session folder.
    if (!cleaned || /^\.+$/.test(cleaned)) throw new Error('Invalid --session value after sanitization');
    return cleaned;
  }
  const u = new URL(targetUrl);
  const stamp = new Date().toISOString().replace(/[-:]/g, '').replace(/\.\d{3}Z$/, 'Z');
  const host = u.hostname.replace(/^www\./, '').replace(/[^A-Za-z0-9.-]/g, '-').slice(0, 40);
  const hash = createHash('sha256').update(targetUrl).digest('hex').slice(0, 8);
  return `${stamp}-${host}-${hash}`;
}

const VALUE_FLAGS = new Set(['--url', '--mode', '--provider', '--extract-properties', '--max-pages', '--delay-ms', '--max-raw-bytes', '--max-text-bytes',
  '--chunk-bytes', '--cdp-wait-ms', '--out', '--session', '--wait-for', '--proxy-type', '--proxy-country', '--block-resource', '--param',
  '--mock-status', '--mock-body-file', '--mock-content-type', '--mock-credit-cost', '--cdp-port', '--wait-text']);
const BOOLEAN_FLAGS = new Set(['--crawl', '--span-hosts', '--allow-private-host', '--sitemap', '--append', '--no-raw', '--browser', '--cdp-stealth', '--no-cdp-stealth']);

// A mistyped flag would otherwise be ignored and the fetch would run with defaults.
function rejectUnknownFlags(args) {
  for (let i = 0; i < args.length; i += 1) {
    if (VALUE_FLAGS.has(args[i])) i += 1;
    else if (!BOOLEAN_FLAGS.has(args[i])) throw new Error(`Unknown option: ${args[i]} (see fetch.mjs --help)`);
  }
}

function readMode(take) {
  const mode = take('--mode') || 'html';
  if (!MODE_ENDPOINT[mode]) throw new Error('--mode must be html, markdown, extended, or extract');
  const provider = take('--provider') || 'auto';
  // For explicit providers, validate mode compatibility now. For 'auto', defer until env is
  // fetch.mjs resolves 'auto' after reading its process environment.
  let providerDescriptor = null;
  if (provider !== 'auto') {
    providerDescriptor = resolveProvider(provider); // throws on unknown
    if (!providerDescriptor.supportsModes.includes(mode)) throw new Error(`--provider ${provider} does not support --mode ${mode} (supports: ${providerDescriptor.supportsModes.join(', ')})`);
  }
  const extractProperties = take('--extract-properties');
  if (mode === 'extract' && !extractProperties) throw new Error('--mode extract requires --extract-properties');
  return { mode, provider, providerDescriptor, extractProperties };
}

function readCrawl(take, has) {
  const crawl = has('--crawl');
  const maxPages = Number(take('--max-pages') || (crawl ? NaN : 1));
  if (!Number.isSafeInteger(maxPages) || maxPages < 1) throw new Error('--crawl requires --max-pages <n>');
  return { crawl, maxPages };
}

const atLeast = (value, min) => Number.isSafeInteger(value) && value >= min;

function readLimits(take, has) {
  const { crawl, maxPages } = readCrawl(take, has);
  const delayMs = Number(take('--delay-ms') || 2000);
  const maxRawBytes = Number(take('--max-raw-bytes') || 1_000_000);
  const maxTextBytes = Number(take('--max-text-bytes') || 250_000);
  const chunkBytes = Number(take('--chunk-bytes') || 50_000);
  if (!atLeast(maxRawBytes, 0)) throw new Error('--max-raw-bytes must be a non-negative number');
  if (!atLeast(maxTextBytes, 4)) throw new Error('--max-text-bytes must be an integer >= 4');
  if (!atLeast(chunkBytes, 1_000)) throw new Error('--chunk-bytes must be a number >= 1000');
  if (!atLeast(delayMs, 0)) throw new Error('--delay-ms must be a non-negative integer');
  if (take('--cdp-wait-ms') && !atLeast(Number(take('--cdp-wait-ms')), 1)) throw new Error('--cdp-wait-ms must be a positive integer');
  return { crawl, maxPages, delayMs, maxRawBytes, maxTextBytes, chunkBytes };
}

function readOutBase(take) {
  const workspaceRoot = workspaceRootFor(process.cwd());
  const workspaceOutputBase = join(workspaceRoot, '.octocode');
  const outBase = take('--out') ? resolve(take('--out')) : join(workspaceRoot, '.octocode', 'octocode-scraping', 'tmp');
  const outRelative = relative(workspaceOutputBase, outBase);
  if (outRelative.startsWith('..') || isAbsolute(outRelative)) {
    throw new Error(`--out must stay under ${workspaceOutputBase}`);
  }
  return outBase;
}

// Browser, proxy, test-mock, and CDP options: passed through as given.
function passThroughOptions({ take, has, allRepeated }) {
  return {
    browser: has('--browser'),
    waitFor: take('--wait-for') || null,
    proxyType: take('--proxy-type') || null,
    proxyCountry: take('--proxy-country') || null,
    blockResources: allRepeated('--block-resource'),
    passParams: parsePassParams(allRepeated),
    mockStatus: take('--mock-status') || null,
    mockBodyFile: take('--mock-body-file') || null,
    mockContentType: take('--mock-content-type') || 'application/json',
    mockCreditCost: take('--mock-credit-cost') || null,
    cdpPort: take('--cdp-port') || null,
    cdpWaitMs: take('--cdp-wait-ms') ? Number(take('--cdp-wait-ms')) : undefined,
    waitText: take('--wait-text') || null,
    cdpStealth: has('--cdp-stealth') && !has('--no-cdp-stealth'),
  };
}

export function parseConfig(args) {
  const parser = createArgParser(args);
  const { take, has } = parser;
  if (has('--help') || has('-h')) usage(0);
  rejectUnknownFlags(args);
  const targetUrl = take('--url');
  if (!targetUrl) usage(2);
  const target = new URL(targetUrl);
  const { mode, provider, providerDescriptor, extractProperties } = readMode(take);
  const { crawl, maxPages, delayMs, maxRawBytes, maxTextBytes, chunkBytes } = readLimits(take, has);
  const outBase = readOutBase(take);
  const sessionId = safeSessionId(take('--session'), targetUrl);
  return {
    targetUrl,
    mode,
    endpoint: MODE_ENDPOINT[mode],
    provider,
    apiKeyEnv: providerDescriptor?.apiKeyEnv ?? null,
    requiresApiKey: providerDescriptor?.requiresApiKey ?? false,
    extractProperties,
    crawl,
    maxPages,
    delayMs,
    // Crawls stay on the --url host by default; --span-hosts follows links to other public hosts.
    spanHosts: has('--span-hosts'),
    // Only the --url origin is exempt; crawled links and redirects to other private hosts stay refused.
    privateOrigin: has('--allow-private-host') ? target.origin : null,
    sitemap: has('--sitemap'),
    outBase,
    sessionId,
    append: has('--append'),
    maxRawBytes,
    maxTextBytes,
    chunkBytes,
    noRaw: has('--no-raw'),
    ...passThroughOptions(parser),
  };
}
