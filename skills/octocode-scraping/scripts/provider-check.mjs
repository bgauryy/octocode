#!/usr/bin/env node
import { propagateOctocodeEnv } from './octocode-config.mjs';
import { autoSelectProvider, resolveProvider } from './lib/providers.mjs';

const args = process.argv.slice(2);
if (args.includes('--help') || args.includes('-h')) {
  console.log('Usage: provider-check.mjs [--provider direct|cdp|scrapingant]\nNo --provider: report the auto-selected html route. Exit 1 when a required key is missing. Never prints the key.');
  process.exit(0);
}
const providerFlagIndex = args.indexOf('--provider');
const explicit = providerFlagIndex >= 0 ? args[providerFlagIndex + 1] : null;
propagateOctocodeEnv({ cwd: process.cwd(), trusted: true });

// When no explicit --provider given, show which provider auto-selection would pick and why.
if (!explicit) {
  let selected;
  try { selected = autoSelectProvider('html', process.env); } catch (error) {
    console.error(JSON.stringify({ ok: false, error: error.message }, null, 2));
    process.exit(2);
  }
  const provider = resolveProvider(selected);
  const result = {
    auto: true,
    selected: provider.name,
    priority: 'cdp → direct (keyless); scrapingant only via --provider or non-html mode',
  };
  if (provider.requiresApiKey) {
    const present = Boolean(process.env[provider.apiKeyEnv]?.trim());
    Object.assign(result, { apiKeyEnv: provider.apiKeyEnv, key: present ? 'set' : 'missing' });
    console.log(JSON.stringify(result, null, 2));
    process.exit(present ? 0 : 1);
  }
  Object.assign(result, { key: 'not-required' });
  console.log(JSON.stringify(result, null, 2));
  process.exit(0);
}

// Explicit --provider: original behavior.
let provider;
try { provider = resolveProvider(explicit); } catch (error) {
  console.error(JSON.stringify({ ok: false, error: error.message }, null, 2));
  process.exit(2);
}
if (!provider.requiresApiKey) {
  console.log(JSON.stringify({ provider: provider.name, key: 'not-required' }, null, 2));
  process.exit(0);
}
const present = Boolean(process.env[provider.apiKeyEnv]?.trim());
console.log(JSON.stringify({ provider: provider.name, apiKeyEnv: provider.apiKeyEnv, key: present ? 'set' : 'missing' }, null, 2));
process.exit(present ? 0 : 1);
