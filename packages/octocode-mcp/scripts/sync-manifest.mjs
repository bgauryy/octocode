#!/usr/bin/env node
/**
 * Derived, never authored: the DXT manifest's `tools` list (every tool the
 * public catalog exposes over MCP, with core's short description and the gate
 * env var from the config contract) and the release version of manifest.json
 * and server.json, which is package.json's.
 *
 *   node scripts/sync-manifest.mjs          rewrite manifest.json and server.json
 *   node scripts/sync-manifest.mjs --check  fail when either is stale
 */
import { readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { configFieldEnvNames } from '@octocodeai/config';
import { getPublicToolCatalog, TOOL_NAMES } from '@octocodeai/config/schema';

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

const packageRoot = resolve(import.meta.dirname, '..');
const { version } = JSON.parse(
  readFileSync(resolve(packageRoot, 'package.json'), 'utf8')
);
const derived = {
  'manifest.json': manifest => ({
    ...manifest,
    version,
    tools: manifestTools(),
  }),
  'server.json': server => ({
    ...server,
    version,
    packages: server.packages.map(entry =>
      entry.identifier === 'octocode-mcp' ? { ...entry, version } : entry
    ),
  }),
};

let stale = false;
for (const [name, derive] of Object.entries(derived)) {
  const path = resolve(packageRoot, name);
  const source = readFileSync(path, 'utf8');
  const next = `${JSON.stringify(derive(JSON.parse(source)), null, 2)}\n`;
  if (next === source) {
    console.log(`✓ ${name} matches the public catalog and package version`);
  } else if (check) {
    console.error(
      `${name} differs from the public catalog or package version; run \`node scripts/sync-manifest.mjs\` in packages/octocode-mcp.`
    );
    stale = true;
  } else {
    writeFileSync(path, next);
    console.log(`✓ ${name} synced from the public catalog and package version`);
  }
}
if (stale) process.exit(1);
