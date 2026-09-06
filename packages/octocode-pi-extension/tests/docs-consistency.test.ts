import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { test } from 'vitest';
import { listExtensionHarness } from '../src/index.js';
import { OCTOCODE_SUPPORT_TOOL_NAMES, OVERRIDDEN_BUILTIN_TOOL_NAMES } from '../src/constants.js';

const packageRoot = path.resolve(import.meta.dirname, '..');

function readPackageFile(relativePath: string): string {
  return fs.readFileSync(path.join(packageRoot, relativePath), 'utf8');
}

test('README surface counts match the harness source', () => {
  const readme = readPackageFile('README.md');
  const harness = listExtensionHarness(packageRoot);

  assert.match(
    readme,
    new RegExp(`\\| Pi support tools \\| ${OCTOCODE_SUPPORT_TOOL_NAMES.length} \\|`)
  );
  assert.match(
    readme,
    new RegExp(`\\| Guarded Pi builtin overrides \\| ${OVERRIDDEN_BUILTIN_TOOL_NAMES.length} \\(`)
  );
  assert.match(
    readme,
    new RegExp(`\\| Slash command entries \\| ${harness.extensionCommands.length} \\|`)
  );
  assert.match(
    readme,
    new RegExp(`## Slash command entries \\(${harness.extensionCommands.length}\\)`)
  );
});

test('support tool inventory exposes only the consolidated coordination surface', () => {
  for (const name of ['agent', 'callTool', 'skill', 'plan', 'localServer', 'lock', 'message']) {
    assert.ok(OCTOCODE_SUPPORT_TOOL_NAMES.includes(name as never), `${name} missing from support inventory`);
  }
  for (const name of ['awarenessStatus', 'awarenessPlan', 'claim', 'task', 'handoff', 'verify', 'awarenessAgents']) {
    assert.equal(OCTOCODE_SUPPORT_TOOL_NAMES.includes(name as never), false, `${name} must not have a public alias`);
  }
});

test('agent-facing docs expose only the consolidated media surface', () => {
  for (const relativePath of ['README.md', 'HARNESS.md', 'docs/TOOLS.md']) {
    const content = readPackageFile(relativePath);
    assert.ok(content.includes('`readMedia`'), `readMedia missing from ${relativePath}`);
    assert.ok(content.includes('`media`'), `media missing from ${relativePath}`);
    assert.doesNotMatch(content, /\| `readImage` \||\| `createMedia` \|/, `${relativePath} advertises a retired media tool`);
  }
});

test('agent-facing docs expose file instead of public edit/write tools', () => {
  for (const relativePath of ['README.md', 'HARNESS.md', 'docs/TOOLS.md', 'docs/OVERRIDES.md']) {
    const content = readPackageFile(relativePath);
    assert.ok(content.includes('`file`'), `file missing from ${relativePath}`);
    assert.doesNotMatch(content, /\| `edit` \| Keep|\| `write` \| Keep/, `${relativePath} retains edit/write as public tools`);
  }
});

test('README command table lists every harness command entry', () => {
  const readme = readPackageFile('README.md');
  const harness = listExtensionHarness(packageRoot);

  for (const command of harness.extensionCommands) {
    assert.ok(readme.includes(`\`${command}\``), `${command} missing from README`);
  }
});

test('settings control-center reference covers every implemented domain and is indexed', () => {
  const settings = readPackageFile('docs/SETTINGS.md');
  const docsIndex = readPackageFile('docs/README.md');
  const harness = readPackageFile('HARNESS.md');
  const rootMcp = readPackageFile('../../docs/OCTOCODE_MCP.md');

  for (const heading of [
    '## Commands',
    '## MCP connections and tools',
    '## Add or edit a managed MCP server',
    '## Discovery sources',
    '## Agent context and prompt artifacts',
    '## Skills',
    '## Overrides and persistence',
    '## Security model',
    '## Refresh and lifecycle behavior',
    '## Current boundaries',
  ]) {
    assert.ok(settings.includes(heading), `${heading} missing from SETTINGS.md`);
  }

  for (const contract of [
    '`pi.getCommands()`',
    '`mcp_server_overrides`',
    '`mcp_tool_overrides`',
    '`skill_overrides`',
    '`catalog.json`',
    '`mcp.md`',
    '`x-octocode-action-token`',
    '`Cache-Control: no-store`',
    '`X-Content-Type-Options: nosniff`',
    '`/new`',
  ]) {
    assert.ok(settings.includes(contract), `${contract} missing from SETTINGS.md`);
  }

  assert.ok(rootMcp.length > 0, 'root MCP guide is present');
  for (const [label, content] of [['docs index', docsIndex], ['harness', harness]] as const) {
    assert.ok(content.includes('SETTINGS.md'), `${label} links to the settings reference`);
  }
});

test('TOOLS browser guidance uses the unified agent facade', () => {
  const tools = readPackageFile('docs/TOOLS.md');

  assert.match(tools, /`agent`.*browser.*lifecycle/is);
  assert.doesNotMatch(tools, /browserAgent\(\.\.\.\).*spawnAgent\(\.\.\.\).*AgentMessage\(\.\.\.\)/s);
});

test('browser skill uses the current host-neutral agent facade', () => {
  const skill = readPackageFile('subagents/browser-agent/skills/browser-agent/SKILL.md');
  const operations = [...skill.matchAll(/type:\s*"([^"]+)"/g)].map((match) => match[1]);

  assert.match(skill, /agent\(\{queries:/);
  assert.match(skill, /profile:\s*"browser"/);
  assert.deepEqual(
    [...new Set(operations)].sort(),
    ['abort', 'inspect', 'kill', 'message', 'spawn', 'wait'].sort(),
  );
  assert.doesNotMatch(skill, /\b(?:browserAgent|spawnAgent|AgentMessage)\b/);
  assert.doesNotMatch(skill, /\bPi\b|\bpi\s+-/);
});

test('README bundled-skill count and names match the canonical bundle inventory', () => {
  const readme = readPackageFile('README.md');
  const skills = fs.readdirSync(path.join(packageRoot, 'dist/skills'), { withFileTypes: true })
    .filter(entry => entry.isDirectory() && fs.existsSync(path.join(packageRoot, 'dist/skills', entry.name, 'SKILL.md')))
    .map(entry => entry.name).sort();
  assert.ok(skills.length > 0, 'build must provide the actual bundled skill inventory');
  const section = readme.split(/## Bundled skills[^\n]*\n/)[1]?.split(/\n## /)[0] ?? '';
  const documented = [...section.matchAll(/^- `(octocode-[^`]+)`/gm)].map(match => match[1]).sort();
  assert.deepEqual(documented, skills, 'README lists exactly the skills shipped by the build');
  assert.match(readme, new RegExp(`## Bundled skills \\(${skills.length}\\)`));
  assert.match(readme, new RegExp(`\\| Bundled main-agent skills \\| ${skills.length} \\|`));
  for (const skill of skills) {
    assert.ok(readme.includes(`\`${skill}\``), `${skill} missing from README bundled-skill inventory`);
  }
  assert.doesNotMatch(readme, /`octocode-awareness` is copied|^- `octocode-awareness`$/m);
});

test('HARNESS summary counts match stable source contracts', () => {
  const harnessDoc = readPackageFile('HARNESS.md');

  assert.match(harnessDoc, new RegExp(`\n${OCTOCODE_SUPPORT_TOOL_NAMES.length}  support tools`));
  assert.match(harnessDoc, /\n 5  worker profiles/);
  assert.match(harnessDoc, /\n 1  composed system prompt/);
  assert.match(harnessDoc, /shared Octocode policy.*Awareness coordination.*Pi-specific `<engineering>` delta/is);
});


test('HARNESS and UI inventories derive from the current extension harness', () => {
  const harnessDoc = readPackageFile('HARNESS.md');
  const uiDoc = readPackageFile('docs/UI.md');
  const harness = listExtensionHarness(packageRoot);

  assert.match(harnessDoc, new RegExp(`### Support Tools — ${OCTOCODE_SUPPORT_TOOL_NAMES.length}\\b`));
  for (const tool of harness.supportTools) {
    assert.ok(harnessDoc.includes(`\`${tool}\``), `${tool} missing from HARNESS support inventory`);
  }
  assert.match(harnessDoc, new RegExp(`\n${harness.extensionCommands.length}  slash commands`));
  assert.match(harnessDoc, /researcher, architect, and planner profiles use standalone prompts/);
  for (const profile of ['researcher', 'architect', 'planner', 'browser', 'custom']) {
    assert.ok(harnessDoc.includes(`\`${profile}\``), `${profile} missing from HARNESS agent profiles`);
  }
  assert.doesNotMatch(harnessDoc, /spawnSubagent[^\n]*browser-agent/);
  assert.doesNotMatch(harnessDoc, /bundles its skill assets|bundled skill\s+\(octocode-awareness/);

  assert.match(uiDoc, new RegExp(`✓ tools: 0 native Pi tools \\+ ${OCTOCODE_SUPPORT_TOOL_NAMES.length} support tools`));
  assert.doesNotMatch(uiDoc, /13 native Pi tools \+ 7 support tools/);
});

test('TUI permutation contract preserves the complete interactive surface', () => {
  const contract = readPackageFile('docs/TUI_PERMUTATION_CONTRACT.html');

  for (const requiredCopy of [
    'Octocode TUI permutation contract',
    'Compact',
    'Default',
    'Full',
    'Light',
    'Dark',
    'Agents',
    'Plan and tasks',
    'Awareness',
    'Mutating tools',
    'Independent reads',
    'Tool results',
    'Compaction checkpoint',
    'Awareness handoff',
    'Media · TUI protocol and browser fallback',
    'Ghostty',
    'browser; ask first',
  ]) {
    assert.ok(contract.includes(requiredCopy), `${requiredCopy} missing from TUI permutation contract`);
  }

  assert.match(contract, /queryRunType/);
  assert.match(contract, /aria-(?:label|pressed)/);
  assert.doesNotMatch(contract, /◆ Octocode/);

  const ids = [...contract.matchAll(/\sid="([^"]+)"/g)].map((match) => match[1]);
  assert.equal(new Set(ids).size, ids.length, 'TUI permutation contract contains duplicate element IDs');
});
