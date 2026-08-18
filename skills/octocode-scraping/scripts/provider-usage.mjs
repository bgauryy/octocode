#!/usr/bin/env node
import { propagateOctocodeEnv } from './octocode-config.mjs';

const args = process.argv.slice(2);
if (args.includes('--help') || args.includes('-h')) {
  console.log('Usage: provider-usage.mjs\nReports ScrapingAnt plan and remaining credits from /v2/usage. Needs SCRAPING_ANT; sanitized output, never prints the key.');
  process.exit(0);
}
propagateOctocodeEnv({ cwd: process.cwd(), trusted: true });
const apiKey = process.env.SCRAPING_ANT?.trim();
if (!apiKey) {
  console.error(JSON.stringify({ ok: false, provider: 'scrapingant', error: 'SCRAPING_ANT missing' }, null, 2));
  process.exit(1);
}

const url = new URL('https://api.scrapingant.com/v2/usage');
url.searchParams.set('x-api-key', apiKey);
const res = await fetch(url, { headers: { 'user-agent': 'octocode-scraping-usage/0.1' } });
const text = await res.text();
let data;
try { data = JSON.parse(text); } catch { data = { detail: text.slice(0, 500) }; }
const safe = {
  ok: res.ok,
  provider: 'scrapingant',
  status: res.status,
  planName: data.plan_name ?? data.planName ?? null,
  startDate: data.start_date ?? null,
  endDate: data.end_date ?? null,
  planTotalCredits: data.plan_total_credits ?? null,
  creditsUsed: data.credits_used ?? data.used_credits ?? null,
  creditsRemaining: data.credits_remaining ?? data.remaining_credits ?? null,
  detail: data.detail ?? null
};
console.log(JSON.stringify(safe, null, 2));
process.exit(res.ok ? 0 : 1);
