import { createHash } from 'node:crypto';
import { resolveProvider } from './providers.mjs';

export const MODE_ENDPOINT = { html: 'general', markdown: 'markdown', extended: 'extended', extract: 'extract' };

export function createArgParser(args) {
  const usage = (exitCode = 2) => {
    console.error(`Usage: scrapingant-fetch.mjs --url <url> [--provider scrapingant|direct|cdp] [--mode html|markdown|extended|extract] [--extract-properties <text>] [--crawl --max-pages <n> [--sitemap] [--same-domain] [--delay-ms <n>]] [--session <id>] [--out <dir>] [--browser] [--wait-for <selector>] [--no-raw] [--max-raw-bytes <n>] [--max-text-bytes <n>] [--chunk-bytes <n>] [--extract-links] [--param k=v] [--cdp-port <n>] [--cdp-wait-ms <n>] [--no-cdp-stealth]\nDefault output: .octocode/tmp/scrape/<sessionId>\n`);
    process.exit(exitCode);
  };
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
    if (!cleaned) throw new Error('Invalid --session value after sanitization');
    return cleaned;
  }
  const u = new URL(targetUrl);
  const stamp = new Date().toISOString().replace(/[-:]/g, '').replace(/\.\d{3}Z$/, 'Z');
  const host = u.hostname.replace(/^www\./, '').replace(/[^A-Za-z0-9.-]/g, '-').slice(0, 40);
  const hash = createHash('sha256').update(targetUrl).digest('hex').slice(0, 8);
  return `${stamp}-${host}-${hash}`;
}

export function parseConfig(args) {
  const parser = createArgParser(args);
  const { take, has, allRepeated, usage } = parser;
  if (has('--help') || has('-h')) usage(0);
  const targetUrl = take('--url');
  if (!targetUrl) usage(2);
  new URL(targetUrl);
  const mode = take('--mode') || 'html';
  if (!MODE_ENDPOINT[mode]) throw new Error('--mode must be html, markdown, extended, or extract');
  const provider = take('--provider') || 'auto';
  // For explicit providers, validate mode compatibility now. For 'auto', defer until env is
  // propagated (scrapingant-fetch.mjs resolves 'auto' → real provider after propagateOctocodeEnv).
  let providerDescriptor = null;
  if (provider !== 'auto') {
    providerDescriptor = resolveProvider(provider); // throws on unknown
    if (!providerDescriptor.supportsModes.includes(mode)) throw new Error(`--provider ${provider} does not support --mode ${mode} (supports: ${providerDescriptor.supportsModes.join(', ')})`);
  }
  const extractProperties = take('--extract-properties');
  if (mode === 'extract' && !extractProperties) throw new Error('--mode extract requires --extract-properties');
  const crawl = has('--crawl');
  const maxPages = Number(take('--max-pages') || (crawl ? NaN : 1));
  if (crawl && (!Number.isFinite(maxPages) || maxPages < 1)) throw new Error('--crawl requires --max-pages <n>');
  const delayMs = Number(take('--delay-ms') || 2000);
  const maxRawBytes = Number(take('--max-raw-bytes') || 1_000_000);
  const maxTextBytes = Number(take('--max-text-bytes') || 250_000);
  const chunkBytes = Number(take('--chunk-bytes') || 50_000);
  if (!Number.isFinite(maxRawBytes) || maxRawBytes < 0) throw new Error('--max-raw-bytes must be a non-negative number');
  if (!Number.isFinite(maxTextBytes) || maxTextBytes < 1) throw new Error('--max-text-bytes must be a positive number');
  if (!Number.isFinite(chunkBytes) || chunkBytes < 1_000) throw new Error('--chunk-bytes must be a number >= 1000');
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
    sameDomain: has('--same-domain'),
    sitemap: has('--sitemap'),
    outBase: take('--out') || '.octocode/tmp/scrape',
    sessionId: safeSessionId(take('--session'), targetUrl),
    maxRawBytes,
    maxTextBytes,
    chunkBytes,
    noRaw: has('--no-raw'),
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
    cdpStealth: !has('--no-cdp-stealth')
  };
}
