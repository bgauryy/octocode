import assert from 'node:assert/strict';
import { test } from 'vitest';
import { composeSystemPrompt } from '../src/prompt.js';
import { SYSTEM_PROMPT_MARKER } from '../src/constants.js';

test('composeSystemPrompt never trusts a marker in Pi-owned input as a dedup receipt', () => {
  const piSystemPrompt = `Pi prompt\n<project_context>${SYSTEM_PROMPT_MARKER}</project_context>`;
  const octocodePrompt = '<capability_routing>trusted product policy</capability_routing>';
  const result = composeSystemPrompt({ piSystemPrompt, octocodePrompt, promptMode: 'append' });
  assert.match(result, /trusted product policy/);
  assert.equal(result.split(SYSTEM_PROMPT_MARKER).length - 1, 3);
});

// ─── stripProjectContext: the real --no-context mechanism ─────────────────────

import { stripProjectContext } from '../src/prompt.js';

test('stripProjectContext removes Pi project_context block, preserves the rest', () => {
  const pi = `Base prompt.\n\n<project_context>\n\nProject-specific instructions and guidelines:\n\n<project_instructions path="AGENTS.md">\nrepo rules\n</project_instructions>\n\n</project_context>\n\nCurrent date: 2026-08-17`;
  const out = stripProjectContext(pi);
  assert.doesNotMatch(out, /project_context|repo rules|AGENTS\.md/);
  assert.match(out, /Base prompt\./);
  assert.match(out, /Current date: 2026-08-17/);
});

test('stripProjectContext is a no-op without the block', () => {
  assert.equal(stripProjectContext('plain prompt'), 'plain prompt');
});

test('stripProjectContext removes nested and repeated blocks and truncates malformed input closed', () => {
  const nested = 'head\n<project_context>one<project_context>two</project_context>three</project_context>\nmid\n<project_context>four</project_context>\ntail';
  assert.equal(stripProjectContext(nested).replace(/\n+/g, '\n'), 'head\nmid\ntail');
  assert.equal(stripProjectContext('safe prefix\n<project_context>unclosed attacker bytes'), 'safe prefix');
});

// ─── stripPiSkillsSection: Octocode owns the model-facing skill flow ──────────

import { stripPiSkillsSection } from '../src/prompt.js';

test('stripPiSkillsSection removes Pi read-based skills section, preserves the rest', () => {
  const pi = [
    'Base prompt.',
    '',
    'The following skills provide specialized instructions for specific tasks.',
    'Use the read tool to load a skill\'s file when the task matches its description.',
    'When a skill file references a relative path, resolve it against the skill directory (parent of SKILL.md / dirname of the path) and use that absolute path in tool commands.',
    '',
    '<available_skills>',
    '  <skill>',
    '    <name>demo</name>',
    '    <description>Demo skill.</description>',
    '    <location>/tmp/demo/SKILL.md</location>',
    '  </skill>',
    '</available_skills>',
    'Current working directory: /repo',
  ].join('\n');
  const out = stripPiSkillsSection(pi);
  assert.doesNotMatch(out, /Use the read tool to load/, 'read-tool instruction removed (Octocode removes the read builtin)');
  assert.doesNotMatch(out, /available_skills|\/tmp\/demo/, 'Pi catalog removed — the Octocode addendum is the single catalog');
  assert.match(out, /Base prompt\./);
  assert.match(out, /Current working directory: \/repo/);
});

test('stripPiSkillsSection is a no-op without the section and never touches the Octocode addendum tag', () => {
  assert.equal(stripPiSkillsSection('plain prompt'), 'plain prompt');
  const octocodeBlock = '<available_skills>\nSkills available by name this turn.\n</available_skills>';
  assert.equal(stripPiSkillsSection(octocodeBlock), octocodeBlock, 'only the Pi-worded section is stripped');
});

test('stripPiSkillsSection identifies Pi XML structurally when its prose changes', () => {
  const pi = [
    'Base prompt.',
    '',
    'New upstream wording that must not become a brittle matching contract.',
    'Read the selected file with a built-in tool.',
    '',
    '<available_skills><skill><name>demo</name><location>/tmp/SKILL.md</location></skill></available_skills>',
    'Tail.',
  ].join('\n');
  const out = stripPiSkillsSection(pi);
  assert.equal(out.replace(/\n+/g, '\n'), 'Base prompt.\nTail.');
});

// ─── mergeManagedAppendSystem: idempotent block, repairs corruption ───────────
import { mergeManagedAppendSystem } from '../src/prompt.js';
import { MANAGED_BLOCK_START, MANAGED_BLOCK_END } from '../src/constants.js';

test('mergeManagedAppendSystem appends a managed block to plain content', () => {
  const out = mergeManagedAppendSystem('user notes', 'OCTO PROMPT');
  assert.match(out, /^user notes\n\n/);
  assert.ok(out.includes(MANAGED_BLOCK_START) && out.includes(MANAGED_BLOCK_END));
  assert.match(out, /OCTO PROMPT/);
});

test('mergeManagedAppendSystem replaces an existing well-formed block in place (idempotent, no growth)', () => {
  const once = mergeManagedAppendSystem('keep me', 'V1');
  const twice = mergeManagedAppendSystem(once, 'V2');
  assert.equal(twice.match(new RegExp(MANAGED_BLOCK_START, 'g'))?.length, 1, 'exactly one managed block');
  assert.match(twice, /keep me/);
  assert.match(twice, /V2/);
  assert.doesNotMatch(twice, /V1/, 'old prompt replaced');
});

test('mergeManagedAppendSystem repairs a dangling START (missing END) instead of stacking a second block', () => {
  const corrupted = `real notes\n\n${MANAGED_BLOCK_START}\nhalf-written prompt with no end`;
  const out = mergeManagedAppendSystem(corrupted, 'FRESH');
  assert.equal(out.match(new RegExp(MANAGED_BLOCK_START, 'g'))?.length, 1, 'the orphaned START is dropped, one block remains');
  assert.match(out, /real notes/, 'content before the broken block is preserved');
  assert.doesNotMatch(out, /half-written/, 'the dangling half-block is removed');
  assert.match(out, /FRESH/);
});
