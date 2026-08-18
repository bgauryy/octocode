#!/usr/bin/env node
import { propagateOctocodeEnv } from './octocode-config.mjs';
if (process.argv.includes('--help')) {
  console.log(`serper-search.mjs --check | --presence-only\n\n  Reports whether SERPER_API_KEY is configured; exit 0 when set, 1 when missing.\n  Credentials load from process env, workspace .octocode/.env, then Octocode home via ./octocode-config.mjs.\n  No search output: fetch and search through the host web tool.`);
  process.exit(0);
}
propagateOctocodeEnv({ cwd: process.cwd(), trusted: true });
const has = process.argv.includes('--check') || process.argv.includes('--presence-only');
if (has) {
  console.log(JSON.stringify({ engine: 'serper', key: process.env.SERPER_API_KEY ? 'set' : 'missing' }));
  process.exit(process.env.SERPER_API_KEY ? 0 : 1);
}
console.error('serper-search.mjs only checks configured credentials; use the host web tool for fetching/search output.');
process.exit(2);
