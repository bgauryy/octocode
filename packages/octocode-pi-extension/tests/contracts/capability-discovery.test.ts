import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { afterEach, expect, it } from 'vitest';
import { defaultAgentSkillSources, discoverAgentSkillInventory, effectiveAgentSkills, resolveAgentSkillInventory } from '../../src/contracts/agent-skills.js';
import { discoverMcpSystem } from '../../src/contracts/mcp-discovery.js';

const roots: string[] = [];
afterEach(() => { for (const root of roots.splice(0)) fs.rmSync(root, { recursive: true, force: true }); });
function fixture() {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'capability-discovery-'));
  roots.push(root);
  const home = path.join(root, 'home');
  const cwd = path.join(root, 'repo');
  fs.mkdirSync(path.join(cwd, '.git'), { recursive: true });
  return { root, home, cwd };
}
function write(file: string, text: string) { fs.mkdirSync(path.dirname(file), { recursive: true }); fs.writeFileSync(file, text); }
function skill(file: string, description: string) { write(file, `---\nname: review\ndescription: ${description}\n---\nInstructions.`); }

it('activates native roots while excluding Pi defaults and retaining vendor candidates', () => {
  const { home, cwd } = fixture();
  for (const [base, label] of [
    [path.join(home, '.octocode', 'skills'), 'global'],
    [path.join(cwd, '.agents', 'skills'), 'workspace'],
    [path.join(cwd, '.claude', 'skills'), 'foreign'],
  ]) skill(path.join(base!, 'review', 'SKILL.md'), label!);
  const sources = defaultAgentSkillSources(cwd, home, path.join(home, '.octocode'));
  const inventory = discoverAgentSkillInventory(sources);
  expect(effectiveAgentSkills(inventory.entries).map(s => s.description)).toEqual(['workspace']);
  expect(inventory.entries.find(e => e.vendor === 'claude')?.enabled).toBe(false);
  expect(sources.some(s => s.vendor === 'pi')).toBe(false);
});

it('recursively discovers linked skill directories and terminates realpath cycles', () => {
  const { root } = fixture();
  const skillRoot = path.join(root, 'skills');
  const linked = path.join(root, 'linked');
  skill(path.join(linked, 'review', 'SKILL.md'), 'linked');
  fs.mkdirSync(path.join(skillRoot, 'category'), { recursive: true });
  fs.symlinkSync(linked, path.join(skillRoot, 'category', 'linked'));
  fs.symlinkSync(skillRoot, path.join(linked, 'cycle'));
  const source = { id: 'native', root: skillRoot, vendor: 'octocode' as const, scope: 'user' as const, precedence: 0, defaultEnabled: true };
  const initial = discoverAgentSkillInventory([source]);
  expect(initial.entries).toHaveLength(1);
  expect(initial.entries[0]).toMatchObject({ name: 'review', enabled: true, sourceId: expect.stringMatching(/^sha256:/), revision: expect.stringMatching(/^sha256:/) });
  const replacement = path.join(root, 'replacement');
  skill(path.join(replacement, 'review', 'SKILL.md'), 'linked');
  fs.unlinkSync(path.join(skillRoot, 'category', 'linked'));
  fs.symlinkSync(replacement, path.join(skillRoot, 'category', 'linked'));
  const changed = discoverAgentSkillInventory([source]);
  expect(changed.entries[0]?.sourceId).toBe(initial.entries[0]?.sourceId);
  expect(changed.entries[0]?.revision).not.toBe(initial.entries[0]?.revision);
});

it('protects bundled skill precedence from concrete runtime paths', () => {
  const { root } = fixture();
  const sources = ['bundled', 'runtime'].map((label, precedence) => {
    const base = path.join(root, label);
    skill(path.join(base, 'review', 'SKILL.md'), label);
    return { id: label === 'bundled' ? 'pi:bundled' : 'pi:runtime', vendor: 'pi' as const, scope: 'user' as const, root: base, precedence, defaultEnabled: true };
  });
  const inventory = discoverAgentSkillInventory(sources);
  expect(effectiveAgentSkills(inventory.entries)[0]?.description).toBe('bundled');
});

it.each(['pending-review', 'unavailable', 'disabled', 'untrusted'] as const)('retains an explicit %s selection instead of activating its bundled alternative', (status) => {
  const { root } = fixture();
  const sources = ['bundled', 'selected'].map((label, precedence) => {
    const base = path.join(root, label);
    skill(path.join(base, 'review', 'SKILL.md'), label);
    return { id: label === 'bundled' ? 'pi:bundled' : 'pi:selected', vendor: 'pi' as const, scope: 'user' as const, root: base, precedence, defaultEnabled: true };
  });
  const inventory = discoverAgentSkillInventory(sources);
  const selected = inventory.entries.find(entry => !entry.bundled)!;
  const entries = inventory.entries.map(entry => entry === selected ? {
    ...entry, selected: true, enabled: false, status,
    ...(status === 'unavailable' ? { parseStatus: 'invalid' as const, skill: undefined } : {}),
  } : entry);
  expect(effectiveAgentSkills(entries)).toEqual([]);
  expect(resolveAgentSkillInventory(entries).find(entry => entry.bundled)?.shadowedBy).toBe(selected.sourceId);
});

it('parses full TOML and preserves timeout, filter, environment and disabled import identity', () => {
  const { home, cwd } = fixture();
  const file = path.join(cwd, '.codex', 'config.toml');
  write(file, `[mcp_servers."docs.api"]\ncommand = "docs" # comment\nargs = [\n  "--stdio",\n  "hello\\nworld",\n]\nenv = { TOKEN = "\u0024{env:DOCS_TOKEN}" }\nstartup_timeout_sec = 30\ntool_timeout_sec = 180\nenabled_tools = ["search"]\ndisabled_tools = ["delete"]\ncustom_feature = true\n`);
  const result = discoverMcpSystem(cwd, { homeDir: home });
  expect(result.definitions[0]).toMatchObject({ sourceId: expect.stringMatching(/^sha256:/), revision: expect.stringMatching(/^sha256:/), config: { disabled: true, command: 'docs', args: ['--stdio', 'hello\nworld'], envRefs: { TOKEN: 'DOCS_TOKEN' }, startupTimeoutMs: 30000, timeoutMs: 180000, enabledTools: ['search'], disabledTools: ['delete'] } });
  expect(result.definitions[0]?.diagnostics).toContainEqual(expect.objectContaining({ field: 'custom_feature', code: 'unsupported-field' }));
});

it('marks native MCP roots active and ignores Pi files without writing discovery state', () => {
  const { home, cwd } = fixture();
  const files = [path.join(cwd, '.agents', 'mcp.json'), path.join(home, '.octocode', 'mcp.json'), path.join(home, '.pi', 'agent', 'mcp.json')];
  for (const file of files) write(file, '{"mcpServers":{"docs":{"command":"docs"}}}');
  const result = discoverMcpSystem(cwd, { homeDir: home, env: {} });
  expect(result.configs.filter(c => c.active).map(c => c.path).sort()).toEqual(files.slice(0, 2).sort());
  expect(result.configs.some(c => c.host === 'pi')).toBe(false);
  expect(result.definitions).toEqual([]);
  expect(fs.existsSync(path.join(home, '.octocode', 'agent'))).toBe(false);
});

it('discovers overridden Codex and Claude definitions but ignores Pi MCP overrides', () => {
  const { home, cwd, root } = fixture();
  const codexHome = path.join(root, 'custom-codex');
  const piHome = path.join(root, 'custom-pi');
  write(path.join(codexHome, 'config.toml'), '[mcp_servers.docs]\ncommand="codex-docs"\n');
  write(path.join(piHome, 'mcp.json'), '{"mcpServers":{"pi-docs":{"command":"pi-docs"}}}');
  write(path.join(home, '.claude.json'), JSON.stringify({ mcpServers: { docs: { command: 'global-docs' } }, projects: { [cwd]: { mcpServers: { docs: { command: 'workspace-docs' } } }, '/other': { mcpServers: { unrelated: { command: 'no' } } } } }));
  const result = discoverMcpSystem(cwd, { homeDir: home, env: { CODEX_HOME: codexHome, PI_CODING_AGENT_DIR: piHome } });
  const claude = result.definitions.filter(definition => definition.config.discovered.host === 'claude');
  expect(claude.map(definition => definition.config.command)).toEqual(['global-docs', 'workspace-docs']);
  expect(new Set(claude.map(definition => definition.sourceId)).size).toBe(2);
  expect(claude.map(definition => definition.config.discovered.scope)).toEqual(['user', 'project']);
  expect(result.definitions.some(definition => definition.config.discovered.path === path.join(codexHome, 'config.toml'))).toBe(true);
  expect(result.configs.find(config => config.path === path.join(piHome, 'mcp.json'))).toBeUndefined();
});

it('reports unsupported embedded environment interpolation instead of literal authentication', () => {
  const { home, cwd } = fixture();
  write(path.join(cwd, '.cursor', 'mcp.json'), JSON.stringify({ mcpServers: { docs: { url: 'https://docs.example/mcp', headers: { Authorization: 'Bearer ${env:DOCS_TOKEN}' } } } }));
  const definition = discoverMcpSystem(cwd, { homeDir: home }).definitions[0]!;
  expect(definition.supported).toBe(false);
  expect(definition.diagnostics).toContainEqual(expect.objectContaining({ code: 'unsupported-interpolation', field: 'headers' }));
  expect(definition.rawDefinition.headers).toEqual({ Authorization: 'Bearer ${env:DOCS_TOKEN}' });
});
