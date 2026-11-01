#!/usr/bin/env node
import { fail } from './lib/fail.mjs';
import { loadOctocodeEnv } from './lib/config-env.mjs';

loadOctocodeEnv();

const args = process.argv.slice(2);
if (args.includes('--help') || args.includes('-h')) {
  console.log('Usage: provider-usage.mjs [--check]\nReports ScrapingAnt plan and remaining credits from /v2/usage.\n--check: only report whether SCRAPING_ANT is set ({"key":"set"}), no network call.\nExit 1 when SCRAPING_ANT is missing or the usage call fails; exit 2 on an unknown argument. Never prints the key.');
  process.exit(0);
}
const unknown = args.find((arg) => arg !== '--check');
if (unknown) fail('invalidArguments', `unknown argument ${unknown}`, 2);
const apiKey = process.env.SCRAPING_ANT?.trim();
if (!apiKey) fail('missingKey', 'SCRAPING_ANT missing', 1, { provider: 'scrapingant', apiKeyEnv: 'SCRAPING_ANT', key: 'missing' });
if (args.includes('--check')) {
  console.log(JSON.stringify({ ok: true, provider: 'scrapingant', apiKeyEnv: 'SCRAPING_ANT', key: 'set' }, null, 2));
  process.exit(0);
}

const url = new URL('https://api.scrapingant.com/v2/usage');
url.searchParams.set('x-api-key', apiKey);
const encodedKey = url.searchParams.toString().slice('x-api-key='.length);
const maskKey = (value) => String(value).replaceAll(apiKey, '[REDACTED]').replaceAll(encodeURIComponent(apiKey), '[REDACTED]').replaceAll(encodedKey, '[REDACTED]');
let res, body;
try {
  res = await fetch(url, { headers: { 'user-agent': 'octocode-scraping-usage/0.1' }, signal: AbortSignal.timeout(20_000) });
  body = await res.text();
} catch (error) {
  fail('providerUnavailable', maskKey(error?.message || error), 1, { provider: 'scrapingant' });
}
let data;
try { data = JSON.parse(body); } catch { fail('invalidResponse', 'usage response is not valid JSON', 1, { provider: 'scrapingant', status: res.status }); }
if (!data || typeof data !== 'object' || Array.isArray(data)) {
  fail('invalidResponse', 'usage response must be a JSON object', 1, { provider: 'scrapingant', status: res.status });
}
const safeScalar = (value) => typeof value === 'string' ? maskKey(value) : Number.isFinite(value) ? value : null;
const safe = {
  ok: res.ok,
  provider: 'scrapingant',
  status: res.status,
  planName: safeScalar(data.plan_name ?? data.planName),
  startDate: safeScalar(data.start_date),
  endDate: safeScalar(data.end_date),
  planTotalCredits: safeScalar(data.plan_total_credits),
  creditsUsed: safeScalar(data.credits_used ?? data.used_credits),
  creditsRemaining: safeScalar(data.credits_remaining ?? data.remaining_credits),
  detail: safeScalar(data.detail)
};
console.log(JSON.stringify(safe, null, 2));
process.exit(res.ok ? 0 : 1);
