#!/usr/bin/env node
/**
 * The DXT manifest's `tools` list is derived, never authored: every tool the
 * public catalog exposes over MCP (core policy excludes CLI-only tools), with
 * core's short description and the gate env var from the config contract.
 *
 *   node scripts/sync-manifest.mjs          rewrite manifest.json
 *   node scripts/sync-manifest.mjs --check  fail when manifest.json is stale
 */
import { readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { configFieldEnvNames } from '@octocodeai/config';
import { getPublicToolCatalog, TOOL_NAMES } from '@octocodeai/config/schema';

const manifestPath = resolve(import.meta.dirname, '..', 'manifest.json');
const check = process.argv.includes('--check');

function gateEnv(fieldPath) {
  const [name] = configFieldEnvNames(fieldPath);
  if (!name) throw new Error(`config field ${fieldPath} has no env binding`);
  return name;
}

const betaEnv = gateEnv('local.beta');
const classificationEnv = gateEnv('classification.api');

function describe(tool) {
  const text = tool.shortDescription.replace(/\.$/, '');
  if (tool.beta) return `${text} (beta: requires ${betaEnv})`;
  if (tool.name === TOOL_NAMES.CLASIFY)
    return `${text} (conditional: requires ${classificationEnv})`;
  return text;
}

function manifestTools() {
  return getPublicToolCatalog()
    .tools.filter(tool => !tool.cliOnly)
    .map(tool => ({ name: tool.name, description: describe(tool) }));
}

const source = readFileSync(manifestPath, 'utf8');
const manifest = JSON.parse(source);
const next = `${JSON.stringify({ ...manifest, tools: manifestTools() }, null, 2)}\n`;

if (next === source) {
  console.log('✓ manifest.json tools match the public catalog');
} else if (check) {
  console.error(
    'manifest.json tools differ from the public catalog; run `node scripts/sync-manifest.mjs` in packages/octocode-mcp.'
  );
  process.exit(1);
} else {
  writeFileSync(manifestPath, next);
  console.log('✓ manifest.json tools synced from the public catalog');
}
