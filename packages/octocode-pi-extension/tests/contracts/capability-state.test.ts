import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { afterEach, expect, it } from 'vitest';
import { openOctocodeDb } from '../../src/contracts/db.js';
import { capabilityDefinitionRevision, capabilitySourcePaths, stableCapabilitySourceId } from '../../src/contracts/capability-sources.js';
import { getCapabilitySourceStatus, getSelectedCapabilitySource, listCapabilitySourceReviews, reviewCapabilitySource, setCapabilitySourceEnabled } from '../../src/contracts/capability-state.js';

const roots: string[] = [];
afterEach(() => { for (const root of roots.splice(0)) fs.rmSync(root, { recursive: true, force: true }); });
it('binds reviewed selections to scope, stable identity and exact definition revision', () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'capability-state-')); roots.push(root);
  const db = openOctocodeDb(path.join(root, 'state.sqlite3'));
  const source = { kind: 'skill' as const, sourceId: stableCapabilitySourceId({ kind: 'skill', host: 'claude', scope: 'workspace', path: '/repo/.claude/skills/review/SKILL.md', name: 'review' }), revision: capabilityDefinitionRevision('first'), name: 'review', path: '/repo/.claude/skills/review/SKILL.md' };
  expect(getCapabilitySourceStatus(db, '/repo', source)).toBe('disabled');
  reviewCapabilitySource(db, '/repo', source, { select: true });
  expect(getCapabilitySourceStatus(db, '/repo', source)).toBe('active');
  expect(getCapabilitySourceStatus(db, '/other', source)).toBe('disabled');
  expect(getSelectedCapabilitySource(db, '/repo', 'skill', 'review')).toBe(source.sourceId);
  expect(getCapabilitySourceStatus(db, '/repo', { ...source, revision: capabilityDefinitionRevision('changed') })).toBe('pending-review');
  expect(getCapabilitySourceStatus(db, '/repo', { ...source, available: false })).toBe('unavailable');
  setCapabilitySourceEnabled(db, '/repo', source.sourceId, false);
  expect(getCapabilitySourceStatus(db, '/repo', source)).toBe('disabled');
  expect(listCapabilitySourceReviews(db, '/repo')).toHaveLength(1);
  expect(() => reviewCapabilitySource(db, '/repo', { ...source, revision: 'bad' })).toThrow();
});

it('uses configured homes and stable canonical hashing without discovering credentials', () => {
  const paths = capabilitySourcePaths('/repo', { homeDir: '/home/test', env: { OCTOCODE_HOME: '/custom/octocode', PI_CODING_AGENT_DIR: '/custom/pi', CODEX_HOME: '/custom/codex' } });
  expect(paths.native).toMatchObject({ globalRoot: '/custom/octocode', mcpFile: '/custom/octocode/mcp.json', modelsFile: '/custom/octocode/models.json', skillsDir: '/custom/octocode/skills', hooksDir: '/custom/octocode/hooks', workspaceRoot: '/repo/.agents' });
  expect(paths.pi.agentDir).toBe('/custom/pi');
  expect(paths.codexHome).toBe('/custom/codex');
  expect(capabilityDefinitionRevision({ b: 2, a: 1 })).toBe(capabilityDefinitionRevision({ a: 1, b: 2 }));
});
