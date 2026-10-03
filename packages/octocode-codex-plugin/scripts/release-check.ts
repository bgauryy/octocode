import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { packageRoot, readJson, repoRoot } from './build.ts';

const pkg = readJson(join(packageRoot, 'package.json'));
const catalog = readJson(join(repoRoot, '.agents/plugins/marketplace.json'));
assert.equal(catalog.plugins[0].source.package, pkg.name);
assert.equal(
  catalog.plugins[0].source.version,
  pkg.version,
  'Update the marketplace version before publishing.'
);
const mcp = readJson(join(packageRoot, 'mcp.json')).mcpServers.octocode.args[1];
const onboarding = readFileSync(
  join(packageRoot, 'skills/octocode-get-started/SKILL.md'),
  'utf8'
);
const cli = onboarding.match(
  /npx -y (octocode@\d+\.\d+\.\d+(?:-[\w.-]+)?)/
)?.[1];
assert.ok(cli, 'Onboarding must pin the CLI version.');
for (const spec of [mcp, cli]) {
  try {
    execFileSync('npm', ['view', spec, 'version', '--json'], {
      timeout: 30_000,
      stdio: ['ignore', 'pipe', 'pipe'],
    });
  } catch {
    throw new Error(
      `Cannot verify published runtime ${spec}. Publish and validate the compatible runtime packages first; do not publish this plugin yet.`
    );
  }
}
console.log(
  'Marketplace version and published runtime prerequisites verified.'
);
