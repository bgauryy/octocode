import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { afterEach, expect, it } from 'vitest';
import { openOctocodeDb } from '../src/contracts/db.js';
import { setMcpServerEnabled } from '../src/contracts/mcp-state.js';
import { setSkillEnabled } from '../src/contracts/mcp-state.js';
import { discoverSkillCandidates, discoverSkillStates, discoverSkills, reviewSkillSource } from '../src/tools/skill-discovery.js';
import { loadMcpConfig, projectMcpPath, globalMcpPath, reviewMcpSource, configSignature, upsertServerInFile } from '../src/tools/mcp/config.js';
import { discoverMcpSystem } from '../src/tools/mcp/discovery.js';

const roots: string[] = [];
afterEach(() => { for (const root of roots.splice(0)) fs.rmSync(root, { recursive: true, force: true }); });
function fixture() {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'pi-capability-source-')); roots.push(root);
  const cwd = path.join(root, 'repo'); const homeDir = path.join(root, 'home'); const octocodeHome = path.join(homeDir, '.octocode');
  fs.mkdirSync(path.join(cwd, '.git'), { recursive: true });
  const db = openOctocodeDb(path.join(root, 'state.sqlite3'));
  return { root, cwd, homeDir, octocodeHome, db, bundledDir: path.join(root, 'bundled') };
}
function write(file: string, value: string) { fs.mkdirSync(path.dirname(file), { recursive: true }); fs.writeFileSync(file, value); }
function skill(file: string, description: string) { write(file, `---\nname: review\ndescription: ${description}\n---\nInstructions.`); }

it('keeps changed selected sources pending without falling back to bundled skills', () => {
  const options = fixture();
  const vendorPath = path.join(options.cwd, '.claude', 'skills', 'review', 'SKILL.md');
  skill(path.join(options.bundledDir, 'review', 'SKILL.md'), 'bundled'); skill(vendorPath, 'foreign');
  const candidates = discoverSkillCandidates(options.cwd, [], options.homeDir, options);
  const vendor = candidates.find(candidate => candidate.vendor === 'claude')!;
  expect(vendor).toMatchObject({ enabled: false, status: 'disabled', shadowedBy: expect.any(String) });
  expect(discoverSkills(options.cwd, [], options.homeDir, options).map(skill => skill.description)).toEqual(['bundled']);
  reviewSkillSource(options.cwd, vendor.sourceId, vendor.revision!, 'project', [], options);
  expect(discoverSkills(options.cwd, [], options.homeDir, options).map(skill => skill.description)).toEqual(['foreign']);
  skill(vendorPath, 'changed');
  const changed = discoverSkillCandidates(options.cwd, [], options.homeDir, options);
  expect(changed.find(candidate => candidate.sourceId === vendor.sourceId)).toMatchObject({ status: 'pending-review', selected: true });
  expect(changed.find(candidate => candidate.bundled)?.shadowedBy).toBe(vendor.sourceId);
  expect(discoverSkills(options.cwd, [], options.homeDir, options)).toEqual([]);
  expect(() => reviewSkillSource(options.cwd, vendor.sourceId, vendor.revision!, 'project', [], options)).toThrow(/changed/);
  const bundled = changed.find(candidate => candidate.bundled)!;
  reviewSkillSource(options.cwd, bundled.sourceId, bundled.revision!, 'project', [], options);
  expect(discoverSkills(options.cwd, [], options.homeDir, options).map(skill => skill.description)).toEqual(['bundled']);
});

it('retains removed selected sources as unavailable before resolving bundled alternatives', () => {
  const options = fixture();
  const vendorPath = path.join(options.cwd, '.claude', 'skills', 'review', 'SKILL.md');
  skill(path.join(options.bundledDir, 'review', 'SKILL.md'), 'bundled'); skill(vendorPath, 'foreign');
  const vendor = discoverSkillCandidates(options.cwd, [], options.homeDir, options).find(candidate => candidate.vendor === 'claude')!;
  reviewSkillSource(options.cwd, vendor.sourceId, vendor.revision!, 'project', [], options);
  fs.unlinkSync(vendorPath);
  const unavailable = discoverSkillCandidates(options.cwd, [], options.homeDir, options);
  expect(unavailable.find(candidate => candidate.sourceId === vendor.sourceId)).toMatchObject({ status: 'unavailable', selected: true });
  expect(unavailable.find(candidate => candidate.bundled)?.shadowedBy).toBe(vendor.sourceId);
  expect(discoverSkills(options.cwd, [], options.homeDir, options)).toEqual([]);
  const bundled = unavailable.find(candidate => candidate.bundled)!;
  reviewSkillSource(options.cwd, bundled.sourceId, bundled.revision!, 'project', [], options);
  expect(discoverSkills(options.cwd, [], options.homeDir, options).map(skill => skill.description)).toEqual(['bundled']);
});

it('requires schema-valid concrete Pi paths and keeps workspace skill trust authoritative', () => {
  const options = fixture();
  skill(path.join(options.cwd, '.agents', 'skills', 'review', 'SKILL.md'), 'workspace');
  expect(discoverSkills(options.cwd, [{ name: 'ghost', description: 'missing file' }], options.homeDir, { ...options, trusted: false })).toEqual([]);
});

it('admits only the explicit Pi skill file and chooses the nearest native workspace ancestor', () => {
  const options = fixture();
  const nested = path.join(options.cwd, 'packages', 'app');
  const explicit = path.join(options.root, 'package', 'review', 'SKILL.md');
  skill(explicit, 'runtime');
  write(path.join(options.root, 'package', 'sibling', 'SKILL.md'), '---\nname: sibling\ndescription: not selected\n---\nInstructions.');
  skill(path.join(options.cwd, '.agents', 'skills', 'review', 'SKILL.md'), 'ancestor');
  skill(path.join(nested, '.agents', 'skills', 'review', 'SKILL.md'), 'nearest');
  const selected = discoverSkills(nested, [{ name: 'review', path: explicit }], options.homeDir, options);
  expect(selected.map(entry => entry.description)).toEqual(['nearest']);
  expect(selected.some(entry => entry.name === 'sibling')).toBe(false);
});

it('requires revision review for foreign MCP even when a name-only override is enabled', async () => {
  const options = fixture();
  const file = path.join(options.cwd, '.cursor', 'mcp.json');
  write(file, '{"mcpServers":{"docs":{"command":"docs-mcp"}}}');
  const candidate = discoverMcpSystem(options.cwd, options).definitions[0]!;
  setMcpServerEnabled(options.db, options.cwd, candidate.name, true);
  const context = { cwd: options.cwd, isProjectTrusted: () => true };
  expect((await loadMcpConfig(context, options)).servers.has(candidate.name)).toBe(false);
  reviewMcpSource(options.cwd, candidate.sourceId, candidate.revision, 'project', { ...options, trusted: true });
  expect((await loadMcpConfig(context, options)).servers.has(candidate.name)).toBe(true);
  write(file, '{"mcpServers":{"docs":{"command":"changed-mcp"}}}');
  const changed = await loadMcpConfig(context, options);
  expect(changed.servers.has(candidate.name)).toBe(false);
  expect(changed.configuredServers.get(candidate.name)?.discovered?.reviewStatus).toBe('pending-review');
  expect(() => reviewMcpSource(options.cwd, candidate.sourceId, candidate.revision, 'project', { ...options, trusted: true })).toThrow(/changed/);
});

it('writes public native MCP paths and preserves instructions in drift signatures', async () => {
  const options = fixture();
  expect(projectMcpPath(options.cwd, options.octocodeHome)).toBe(path.join(options.cwd, '.agents', 'mcp.json'));
  expect(globalMcpPath(options.homeDir, options.octocodeHome)).toBe(path.join(options.octocodeHome, 'mcp.json'));
  const file = globalMcpPath(options.homeDir, options.octocodeHome);
  upsertServerInFile(file, 'docs', { command: 'docs', instructions: 'Use docs for package APIs.' });
  const config = (await loadMcpConfig({ cwd: options.cwd }, options)).servers.get('docs')!;
  expect(config.instructions).toBe('Use docs for package APIs.');
  expect(configSignature(config)).not.toBe(configSignature({ ...config, instructions: 'Changed.' }));
});

it('keeps a single disabled skill row when every source for the name is disabled', () => {
  const options = fixture();
  skill(path.join(options.bundledDir, 'review', 'SKILL.md'), 'bundled');
  skill(path.join(options.cwd, '.agents', 'skills', 'review', 'SKILL.md'), 'workspace');
  setSkillEnabled(options.db, options.cwd, 'review', false);
  const rows = discoverSkillStates(options.cwd, [], options.homeDir, options);
  expect(rows).toHaveLength(1);
  expect(rows[0]).toMatchObject({ description: 'bundled', enabled: false });
});

it('preserves existing flat native MCP definitions when adding a server', async () => {
  const options = fixture();
  const file = globalMcpPath(options.homeDir, options.octocodeHome);
  write(file, '{"existing":{"command":"old-server"}}');
  upsertServerInFile(file, 'added', { command: 'new-server' });
  const config = await loadMcpConfig({ cwd: options.cwd }, options);
  expect(config.servers.get('existing')?.command).toBe('old-server');
  expect(config.servers.get('added')?.command).toBe('new-server');
});
