import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { expect, it } from 'vitest';

const packageRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');

it('publishes the complete shared feature and entity inventory', () => {
  const readme = readFileSync(resolve(packageRoot, 'README.md'), 'utf8');
  for (const feature of [
    '`attend`', '`status`, `query`', '`plan`', '`task`', '`work`', '`lock`',
    '`verify`', '`agent`', '`signal`', '`memory`', '`refinement`, `reflect`',
    '`session capture`', '`maintenance`', '`config`, `hooks`, `hook run`',
    '`docs`, `schema`', '`handoff`, `guide`, `instructions export`', 'Library continuity APIs',
  ]) expect(readme).toContain(`| ${feature} |`);
  for (const entity of [
    'plans', 'tasks', 'claims', 'work presence', 'locks', 'verification',
    'messages', 'handoffs', 'signals', 'memory',
  ]) expect(readme.toLowerCase()).toContain(entity);
  expect(readme).toContain('.octocode/awareness.sqlite3');
  expect(readme).toContain('$OCTOCODE_HOME/awareness/awareness.sqlite3');
  expect(readme).toMatch(/Agent control and runtime databases[\s\S]*separate/i);
  expect(readme).toContain('@octocodeai/octocode-awareness');
  expect(readme).toContain('octocode-awareness');
  expect(readme).not.toMatch(/octocode-awareness-lite|\/lite\b|Awareness Lite/);
});
