#!/usr/bin/env node
import { propagateOctocodeEnv } from '@octocodeai/config';
propagateOctocodeEnv({ cwd: process.cwd(), trusted: true });
const has = process.argv.includes('--check') || process.argv.includes('--presence-only');
if (has) {
  console.log(JSON.stringify({ engine: 'exa', key: process.env.EXA_API_KEY ? 'set' : 'missing' }));
  process.exit(process.env.EXA_API_KEY ? 0 : 1);
}
console.error('exa-search.mjs only checks configured credentials; use the host web tool for fetching/search output.');
process.exit(2);
