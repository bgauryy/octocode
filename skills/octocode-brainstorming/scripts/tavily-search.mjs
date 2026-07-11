#!/usr/bin/env node

import { die, loadEnv, splitList, normalizeApiKey } from './lib/web-search-common.mjs';

const ENDPOINT = 'https://api.tavily.com/search';

function parseArgs(argv) {
  const opts = {
    query: '', searchDepth: 'advanced', maxResults: 8, topic: 'general', timeRange: 'year',
    includeDomains: [], excludeDomains: [], autoParameters: false, startDate: '', endDate: '',
    check: false, presenceOnly: false, help: false,
  };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === '--help' || a === '-h') { opts.help = true; continue; }
    if (a === '--check') { opts.check = true; continue; }
    if (a === '--presence-only') { opts.presenceOnly = true; continue; }
    if (a === '--query' || a === '-q') { opts.query = argv[++i] || ''; continue; }
    if (a === '--depth') { opts.searchDepth = argv[++i] || 'advanced'; continue; }
    if (a === '--max-results') { opts.maxResults = Number(argv[++i]) || 8; continue; }
    if (a === '--topic') { opts.topic = argv[++i] || 'general'; continue; }
    if (a === '--time-range') { opts.timeRange = argv[++i] || 'year'; continue; }
    if (a === '--include-domains') { opts.includeDomains = splitList(argv[++i]); continue; }
    if (a === '--exclude-domains') { opts.excludeDomains = splitList(argv[++i]); continue; }
    if (a === '--auto-parameters') { opts.autoParameters = true; continue; }
    if (a === '--start-date') { opts.startDate = argv[++i] || ''; continue; }
    if (a === '--end-date') { opts.endDate = argv[++i] || ''; continue; }
    if (!opts.query) { opts.query = a; continue; }
    die(`Unknown argument: ${a}`); return null;
  }
  // Tavily caps max_results at 20 (0–20); clamp to avoid a 400.
  opts.maxResults = Math.max(0, Math.min(20, opts.maxResults));
  return opts;
}

async function validateKey(apiKey) {
  const res = await fetch(ENDPOINT, {
    method: 'POST',
    headers: { Authorization: `Bearer ${apiKey}`, 'Content-Type': 'application/json' },
    body: JSON.stringify({
      query: 'Tavily API health check',
      search_depth: 'basic',
      max_results: 1,
      include_answer: false,
      include_raw_content: false,
    }),
    signal: AbortSignal.timeout(30_000),
  });

  if (!res.ok) {
    const text = await res.text().catch(() => '');
    throw new Error(`Tavily API ${res.status}: ${text}`);
  }
}

async function search(opts, apiKey) {
  const body = {
    query: opts.query,
    search_depth: opts.searchDepth,
    topic: opts.topic,
    max_results: opts.maxResults,
    include_answer: true,
    include_raw_content: false,
    time_range: opts.timeRange,
  };
  // Optional, all backed by the official Tavily /search contract.
  if (opts.includeDomains.length) body.include_domains = opts.includeDomains;
  if (opts.excludeDomains.length) body.exclude_domains = opts.excludeDomains;
  if (opts.autoParameters) body.auto_parameters = true;
  if (opts.startDate) body.start_date = opts.startDate;
  if (opts.endDate) body.end_date = opts.endDate;

  const res = await fetch(ENDPOINT, {
    method: 'POST',
    headers: { Authorization: `Bearer ${apiKey}`, 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
    signal: AbortSignal.timeout(30_000),
  });

  if (!res.ok) {
    const text = await res.text().catch(() => '');
    die(`Tavily API ${res.status}: ${text}`);
    return null;
  }
  return res.json();
}

async function main() {
  const opts = parseArgs(process.argv.slice(2));
  if (!opts) return;

  if (opts.help) {
    console.log(`Tavily web search — octocode-brainstorming

Usage:
  node tavily-search.mjs --query "query" [options]
  node tavily-search.mjs --check

Options:
  --query, -q        Search query (required unless --check)
  --depth            basic, advanced, fast, or ultra-fast (default: advanced; advanced = 2 credits, deeper extraction)
  --max-results      Number of results, 0–20 (default: 8; clamped to API max of 20)
  --topic            general, news, or finance (default: general)
  --time-range       day, week, month, or year (default: year)
  --include-domains  Comma-separated allowlist, e.g. "docs.python.org,arxiv.org" (quality filter)
  --exclude-domains  Comma-separated blocklist, e.g. "pinterest.com,quora.com" (drop SEO/farm noise)
  --start-date       YYYY-MM-DD lower bound (precise recency)
  --end-date         YYYY-MM-DD upper bound
  --auto-parameters  Let Tavily auto-tune query params for the question
  --check            Validate TAVILY_API_KEY with a live Tavily request
  --presence-only    With --check, only verify a key is present locally

Environment: process env, then <workspace>/.octocode/.env, then <OCTOCODE_HOME>/.env`);
    return;
  }

  const envPath = await loadEnv();
  const apiKey = normalizeApiKey(process.env.TAVILY_API_KEY || process.env.TAVILY_API_TOKEN);

  if (opts.check) {
    if (!apiKey) {
      console.log(`tavily: unavailable (TAVILY_API_KEY not set)`);
      console.log(`Add TAVILY_API_KEY to: ${envPath}`);
      process.exitCode = 1;
      return;
    }
    if (opts.presenceOnly) {
      console.log('tavily: key present (not validated)');
      process.exitCode = 0;
      return;
    }
    try {
      await validateKey(apiKey);
      console.log('tavily: available (validated)');
      process.exitCode = 0;
    } catch (err) {
      console.log('tavily: unavailable (key failed live validation)');
      console.log(err.message || String(err));
      console.log(`Update TAVILY_API_KEY in: ${envPath}`);
      process.exitCode = 1;
    }
    return;
  }

  if (!apiKey) {
    die(`TAVILY_API_KEY is not set. Add it to ${envPath} or export it in your shell.`);
    return;
  }
  if (!opts.query) {
    die('--query is required. Use --help for usage.');
    return;
  }

  process.stderr.write(`Searching Tavily: "${opts.query}" (depth=${opts.searchDepth}, max=${opts.maxResults})\n`);
  const result = await search(opts, apiKey);
  if (result) {
    result.engine = 'tavily';
    console.log(JSON.stringify(result, null, 2));
  }
}

main().catch(err => die(err.message || String(err)));
