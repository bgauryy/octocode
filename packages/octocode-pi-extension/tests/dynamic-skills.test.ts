import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import { afterEach, beforeEach, test } from 'vitest';
import {
  resolveSkill,
  registerSkill,
  validateSkill,
  parseFrontmatter,
  listSkills,
  deleteSkill,
  recordSkillUse,
  sweepJunkSkills,
  readIndex,
  getSkillsDir,
  type SkillManifestEntry,
} from '../src/tools/dynamic-skills.js';

let dir: string;
beforeEach(() => {
  dir = fs.mkdtempSync(path.join(os.tmpdir(), 'callskill-test-'));
});
afterEach(() => {
  fs.rmSync(dir, { recursive: true, force: true });
});

const GOOD = {
  name: 'release-checklist',
  description: 'Run the multi-step release checklist. Use before publishing a package.',
  reason: 'recurring multi-step release workflow',
  skillMd: `---\nname: release-checklist\ndescription: Run the multi-step release checklist. Use before publishing.\n---\n\n# Release Checklist\n\n## Steps\n1. Run tests.\n2. Bump version.\n3. Publish.`,
};
const register = (o: Partial<typeof GOOD> = {}) => registerSkill({ ...GOOD, ...o }, dir);

test('registerSkill writes SKILL.md and indexes a valid skill', () => {
  const res = register();
  assert.equal(res.ok, true);
  assert.ok(fs.existsSync(path.join(dir, 'release-checklist', 'SKILL.md')));
  assert.ok(readIndex(dir).skills['release-checklist']);
});

test('getSkillsDir honors OCTOCODE_DYNAMIC_SKILLS_DIR and defaults under ~/.pi/agent/skills', () => {
  assert.equal(getSkillsDir({ OCTOCODE_DYNAMIC_SKILLS_DIR: '/tmp/x' } as NodeJS.ProcessEnv), '/tmp/x');
  assert.equal(getSkillsDir({ HOME: '/home/u' } as NodeJS.ProcessEnv), path.join('/home/u', '.pi', 'agent', 'skills'));
});

test('parseFrontmatter extracts name and description', () => {
  const fm = parseFrontmatter(GOOD.skillMd);
  assert.equal(fm?.name, 'release-checklist');
  assert.match(String(fm?.description), /release checklist/i);
});

test('resolveSkill returns an exact O(1) hit', () => {
  register();
  assert.equal(resolveSkill('release-checklist', '', dir).hit, 'exact');
});

test('resolveSkill matches via keyword overlap', () => {
  register();
  const r = resolveSkill('publishFlow', 'run the release checklist before publishing', dir);
  assert.equal(r.hit, 'keyword');
});

test('resolveSkill misses unrelated requests', () => {
  register();
  assert.equal(resolveSkill('makeCoffee', 'brew a coffee', dir).hit, 'miss');
});

test('validation rejects an invalid skill name', () => {
  const v = validateSkill({ ...GOOD, name: 'Bad_Name' });
  assert.equal(v.ok, false);
  if (!v.ok) assert.equal(v.reason, 'invalid-name');
});

test('validation rejects a frontmatter name that does not match its directory name', () => {
  const v = validateSkill({ ...GOOD, skillMd: GOOD.skillMd.replace('name: release-checklist', 'name: other-skill') });
  assert.equal(v.ok, false);
  if (!v.ok) assert.equal(v.reason, 'invalid-frontmatter');
});

test('validation rejects a missing reason', () => {
  const v = validateSkill({ ...GOOD, reason: '  ' });
  assert.equal(v.ok, false);
  if (!v.ok) assert.equal(v.reason, 'no-reason');
});

test('validation rejects missing frontmatter', () => {
  const v = validateSkill({ ...GOOD, skillMd: '# No frontmatter\n\n## Steps\n1. do a thing that is long enough to pass length.' });
  assert.equal(v.ok, false);
  if (!v.ok) assert.equal(v.reason, 'invalid-frontmatter');
});

test('validation rejects a too-thin body', () => {
  const v = validateSkill({ ...GOOD, name: 'x-skill', skillMd: `---\nname: x-skill\ndescription: d\n---\n\nno heading` });
  assert.equal(v.ok, false);
  if (!v.ok) assert.equal(v.reason, 'invalid-structure');
});

test('registerSkill rejects invalid skills (validation gate)', () => {
  const res = register({ name: 'BAD' });
  assert.equal(res.ok, false);
});

test('registerSkill writes sandboxed helper files but blocks path escapes', () => {
  const res = registerSkill(
      { ...GOOD, name: 'with-scripts', skillMd: GOOD.skillMd.replace('name: release-checklist', 'name: with-scripts'), files: [{ relPath: 'scripts/run.sh', content: 'echo hi' }, { relPath: '../escape.sh', content: 'bad' }] },
    dir,
  );
  assert.ok(res.ok);
  assert.ok(fs.existsSync(path.join(dir, 'with-scripts', 'scripts', 'run.sh')));
  assert.equal(fs.existsSync(path.join(dir, 'escape.sh')), false);
});

test('re-registration bumps version and preserves createdAt', () => {
  const first = register();
  assert.ok(first.ok);
  const created = (first as { entry: SkillManifestEntry }).entry.createdAt;
  const second = register({ description: 'Updated release checklist. Use before publishing a package now.' });
  assert.ok(second.ok);
  if (second.ok) {
    assert.equal(second.entry.version, 2);
    assert.equal(second.entry.createdAt, created);
  }
});

test('listSkills and deleteSkill CRUD', () => {
  register();
  assert.equal(listSkills(dir).length, 1);
  assert.equal(deleteSkill('release-checklist', dir), true);
  assert.equal(listSkills(dir).length, 0);
  assert.equal(deleteSkill('release-checklist', dir), false);
});

test('recordSkillUse increments usage', () => {
  register();
  recordSkillUse('release-checklist', dir);
  assert.equal(readIndex(dir).skills['release-checklist'].stats.uses, 1);
});

test('sweepJunkSkills prunes an entry whose SKILL.md was deleted', () => {
  register();
  fs.rmSync(path.join(dir, 'release-checklist', 'SKILL.md'));
  const pruned = sweepJunkSkills(dir);
  assert.deepEqual(pruned, ['release-checklist']);
  assert.equal(readIndex(dir).skills['release-checklist'], undefined);
});
