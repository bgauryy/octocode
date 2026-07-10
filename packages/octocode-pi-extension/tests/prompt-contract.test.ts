import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { test } from 'vitest';
import { SYSTEM_PROMPT } from '../src/prompts/compose.js';

const packageRoot = path.resolve(import.meta.dirname, '..');
const sourceSkillsDir = path.join(packageRoot, 'skills');
const browserSkillDir = path.join(
  packageRoot,
  'subagents',
  'browser-agent',
  'skills',
  'browser-agent'
);

function activeSkillCatalog(): string[] {
  const content = fs.readFileSync(
    path.join(packageRoot, 'src', 'prompts', 'sections', 'skills.md'),
    'utf8'
  );
  return [...content.matchAll(/^- `([a-z0-9-]+)` —/gm)].map(
    match => match[1]!
  );
}

test('coder prompt exposes an explicit delegation and worker lifecycle contract', () => {
  assert.match(SYSTEM_PROMPT, /Delegation gate \(before spawning\)/);
  assert.match(SYSTEM_PROMPT, /Worker request packet \(required\)/);
  assert.match(SYSTEM_PROMPT, /workers inherit no parent conversation/i);
  assert.match(SYSTEM_PROMPT, /share the current `cwd`, filesystem/);
  assert.match(SYSTEM_PROMPT, /current turn to become idle or terminal/);
  assert.match(SYSTEM_PROMPT, /does not prove the delegated objective is complete/);
  assert.match(SYSTEM_PROMPT, /acceptance criteria pass/);
  assert.match(SYSTEM_PROMPT, /steer` once/);

  assert.doesNotMatch(SYSTEM_PROMPT, /octocode-subagents/);
  assert.doesNotMatch(SYSTEM_PROMPT, /No shared state between workers/);
  assert.doesNotMatch(SYSTEM_PROMPT, /Prompt is the only channel/);
  assert.doesNotMatch(SYSTEM_PROMPT, /block until done/);
  assert.doesNotMatch(SYSTEM_PROMPT, /2 failed steers|correction failure 2/);
});

test('coder prompt keeps safety and delegation before detailed tool routing', () => {
  const safety = SYSTEM_PROMPT.indexOf('<safety>');
  const agents = SYSTEM_PROMPT.indexOf('<agents>');
  const tools = SYSTEM_PROMPT.indexOf('<tools>');
  const output = SYSTEM_PROMPT.indexOf('<output>');

  assert.ok(safety >= 0 && agents > safety, 'safety precedes delegation');
  assert.ok(agents < tools, 'delegation gate precedes detailed tool routing');
  assert.ok(output > tools, 'output contract remains at the end of execution guidance');
});

test('every active skill catalog entry resolves to a shipped SKILL.md', () => {
  const skills = activeSkillCatalog();
  assert.ok(skills.length > 0, 'skill catalog is not empty');

  for (const skill of skills) {
    const candidates = [
      path.join(sourceSkillsDir, skill, 'SKILL.md'),
      skill === 'browser-agent' ? path.join(browserSkillDir, 'SKILL.md') : '',
    ].filter(Boolean);
    assert.ok(
      candidates.some(candidate => fs.existsSync(candidate)),
      `prompt references missing skill: ${skill}`
    );
  }
});

test('persistence and compaction are conditional instead of mandatory ceremony', () => {
  assert.match(
    SYSTEM_PROMPT,
    /When a plan, RFC, handoff, or research result must outlive the current context/
  );
  assert.match(SYSTEM_PROMPT, /Do not create an artifact for an ordinary answer\/review/);
  assert.match(SYSTEM_PROMPT, /Persist a handoff only when work must survive/);
  assert.doesNotMatch(SYSTEM_PROMPT, /write findings to a doc → compact → execute/);
});
