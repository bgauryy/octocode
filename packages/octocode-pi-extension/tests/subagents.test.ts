import fs from 'node:fs';
import path from 'node:path';
import { parseArgs } from '@earendil-works/pi-coding-agent';
import { describe, expect, it } from 'vitest';
import { addUsage, emptyUsage } from '../src/shared/util.js';
import { assistantText, buildAgentArgs, subagentProcessEnv } from '../src/subagents/process.js';
import { loadProfiles, parseProfile } from '../src/subagents/profiles.js';
import { Subcommands } from '../src/shared/commands.js';
import { collaborateByDefault, MAX_TASK_CHARS, maxSubagents, registerAgentTool, subagentLimitError, teamTask } from '../src/subagents/tool.js';
import { octocodePrompt } from '../src/prompt.js';
import { tmp } from './helpers.js';

describe('subagents', () => {
  it('collaborate mode: off by default, lists teammates in the task, flags the child and its prompt', () => {
    expect(collaborateByDefault({})).toBe(false);
    expect(collaborateByDefault({ OCTOCODE_SUBAGENT_COLLABORATE: 'on' })).toBe(true);
    expect(teamTask('Fix A', 'main-1', [['researcher-2', 'Trace B']])).toBe('Fix A\n\n## Teammates\nYou collaborate with the other subagents that main-1 started for the same goal. Running now:\n- `researcher-2`: Trace B');
    expect(teamTask('Fix A', 'main-1', [])).toMatch(/none yet; `coordinate list` shows teammates that start later/);
    expect(subagentProcessEnv(undefined, {}, { id: 'a-1', collaborate: true })['OCTOCODE_AGENT_COLLABORATE']).toBe('1');
    // The user's command hooks stay in the main session unless the profile opts in.
    expect(subagentProcessEnv(undefined, { OCTOCODE_HOOKS: '1', PATH: '/bin' })).not.toHaveProperty('OCTOCODE_HOOKS');
    expect(subagentProcessEnv(undefined, { OCTOCODE_HOOKS: '1', PATH: '/bin' })['PATH']).toBe('/bin');
    expect(subagentProcessEnv({ name: 'h', description: '', prompt: '', hooks: true }, { OCTOCODE_HOOKS: '1' })['OCTOCODE_HOOKS']).toBe('1');
    expect(subagentProcessEnv({ name: 'h', description: '', prompt: '', hooks: true }, {})).not.toHaveProperty('OCTOCODE_HOOKS');
    expect(parseProfile('x', '---\nhooks: true\n---\nbody').hooks).toBe(true);
    expect(subagentProcessEnv(undefined, {}, { id: 'a-1' })['OCTOCODE_AGENT_COLLABORATE']).toBeUndefined();
    const child = (collaborate: boolean) => octocodePrompt({ octocode: false, profiles: [], canDelegate: false, identity: { id: 'a-1', parentId: 'main-1', collaborate } });
    expect(child(true)).toMatch(/You are on a team/);
    expect(child(false)).not.toMatch(/You are on a team/);
    expect(octocodePrompt({ octocode: false, profiles: [], canDelegate: true })).toMatch(/collaborate: true/);
  });

  it('parses profile frontmatter and builds child pi arguments', () => {
    const profile = parseProfile('fallback', '---\nname: researcher\ndescription: Reads code\ntools: read,grep\n---\nBe precise.');
    expect(profile).toEqual({ name: 'researcher', description: 'Reads code', prompt: 'Be precise.', tools: 'read,grep' });
    expect(buildAgentArgs('Find X', profile, 'anthropic/claude', '/ext/index.js')).toEqual([
      '--mode', 'json', '--no-session', '--no-extensions', '-e', '/ext/index.js', '-e', 'builtin:mcp', '-e', 'builtin:tool-search', '--model', 'anthropic/claude', '--tools', 'read,grep', '--append-system-prompt', 'Be precise.', '--', 'Find X',
    ]);
  });

  it('passes a task that looks like a flag or an @file to Pi as the message, unexpanded', () => {
    const profile = parseProfile('p', '---\ntools: read\n---\n- Be careful.');
    for (const task of ['- fix the bug', '--x', '--model evil/model', '-e /tmp/evil.js', '@src/a.ts', '@src/a.ts then review it', 'plain task']) {
      const parsed = parseArgs(buildAgentArgs(task, profile, 'faux/model', '/ext/index.js'));
      expect(parsed.messages.map((message) => message.trim())).toEqual([task]);
      expect(parsed.fileArgs).toEqual([]);
      expect(parsed.model).toBe('faux/model');
      expect(parsed.extensions).toEqual(['/ext/index.js', 'builtin:mcp', 'builtin:tool-search']);
      expect(parsed.appendSystemPrompt).toEqual(['- Be careful.']);
      expect(parsed.diagnostics).toEqual([]);
    }
  });

  it('loads Pi\'s built-in MCP in children unless the profile turns MCP off', () => {
    const args = buildAgentArgs('X', { name: 'plain', description: '', prompt: '', mcp: false }, undefined, '/ext/index.js');
    expect(args).not.toContain('builtin:mcp');
    expect(args).not.toContain('builtin:tool-search');
  });

  it('maps excludeTools frontmatter to --exclude-tools', () => {
    const profile = parseProfile('reviewer', '---\nexcludeTools: file\n---\nReview.');
    expect(profile.excludeTools).toBe('file');
    expect(buildAgentArgs('Review X', profile, undefined, '/ext/index.js')).toEqual([
      '--mode', 'json', '--no-session', '--no-extensions', '-e', '/ext/index.js', '-e', 'builtin:mcp', '-e', 'builtin:tool-search', '--exclude-tools', 'file,edit,write', '--append-system-prompt', 'Review.', '--', 'Review X',
    ]);
  });

  it('loads project profiles only for trusted projects', () => {
    const cwd = tmp();
    fs.mkdirSync(path.join(cwd, '.pi', 'agents'), { recursive: true });
    fs.writeFileSync(path.join(cwd, '.pi', 'agents', 'local.md'), '---\ndescription: Project profile\n---\nDo it.');
    expect(loadProfiles(cwd, tmp(), false).has('local')).toBe(false);
    expect(loadProfiles(cwd, tmp(), true).has('local')).toBe(true);
  });

  it('offers a headless web profile that never opens a visible browser', () => {
    const headless = loadProfiles(tmp(), tmp()).get('webHeadless');
    expect(headless?.excludeTools?.split(',')).toEqual(['file']);
    // Browser subagents start no MCP servers at all, whatever servers are configured.
    expect(headless?.mcp).toBe(false);
    expect(headless).not.toHaveProperty('visibleBrowser');
    expect(headless?.prompt).toContain('headless Chrome');
    expect(subagentProcessEnv(headless, { PATH: '/usr/bin' })).not.toHaveProperty('OCTOCODE_BROWSER_VISIBLE');
    const prompt = octocodePrompt({ octocode: false, profiles: [{ name: 'webHeadless', description: 'Headless' }, { name: 'webLive', description: 'Visible' }], canDelegate: true });
    expect(prompt).toContain('- webHeadless: Headless');
    expect(prompt).toContain('- webLive: Visible');
  });

  it('bundles exactly the two browser profiles, lean, and restricts read-only profiles', () => {
    const profiles = loadProfiles(tmp(), tmp());
    expect([...profiles.keys()].sort()).toEqual(['implementer', 'researcher', 'reviewer', 'webHeadless', 'webLive']);
    for (const name of ['webHeadless', 'webLive']) expect(profiles.get(name)!.prompt.length, name).toBeLessThanOrEqual(2_000);
    for (const name of ['researcher', 'reviewer']) expect(profiles.get(name)?.excludeTools, name).toBe('file,browser');
    expect(profiles.get('implementer')?.excludeTools).toBe('browser');
  });

  it('loads webLive and marks only that child for a visible browser', () => {
    const profile = loadProfiles(tmp(), tmp()).get('webLive');
    expect(profile).toMatchObject({ name: 'webLive', visibleBrowser: true });
    expect(profile?.excludeTools?.split(',')).toContain('file');
    expect(profile?.prompt).toContain('persistent profile');
    expect(profile?.prompt).not.toContain('cua');
    expect(subagentProcessEnv(profile, { PATH: '/usr/bin' })).toMatchObject({ OCTOCODE_SUBAGENT: '1', OCTOCODE_BROWSER_VISIBLE: '1', PATH: '/usr/bin' });
    expect(subagentProcessEnv(undefined, {})).not.toHaveProperty('OCTOCODE_BROWSER_VISIBLE');
    expect(buildAgentArgs('Open https://example.com', profile, undefined, '/ext/index.js')).toContain('--exclude-tools');
  });

  it('caps running subagents at OCTOCODE_MAX_SUBAGENTS, default 3', () => {
    expect(maxSubagents({})).toBe(3);
    expect(maxSubagents({ OCTOCODE_MAX_SUBAGENTS: '5' })).toBe(5);
    for (const bad of ['0', '-1', '2.5', 'many', '']) expect(maxSubagents({ OCTOCODE_MAX_SUBAGENTS: bad }), bad).toBe(3);
    expect(subagentLimitError(['a', 'b'], 3)).toBeUndefined();
    const refusal = subagentLimitError(['a', 'b', 'c'], 3);
    expect(refusal).toContain('3 subagents are already running (a, b, c)');
    expect(refusal).toContain('OCTOCODE_MAX_SUBAGENTS');
  });

  it('caps the task, offers isolate, and registers /agents with kill and merge', async () => {
    const tools: Array<{ name: string; parameters: { properties: Record<string, { maxLength?: number }> } }> = [];
    const commands = new Map<string, { handler: (args: string, ctx: unknown) => Promise<void> }>();
    const pi = { registerTool: (tool: never) => tools.push(tool), registerCommand: (name: string, command: never) => commands.set(name, command), registerMessageRenderer: () => undefined, on: () => undefined, sendMessage: () => undefined } as never;
    const control = registerAgentTool(pi, () => new Map(), { id: undefined, view: { members: [], traffic: [] }, members: () => [] } as never);
    // index.ts adds it as `/octocode agents` with the `/agents` shortcut.
    const subcommands = new Subcommands();
    subcommands.add('agents', control.command);
    subcommands.alias(pi, 'agents');
    const agent = tools.find((tool) => tool.name === 'agent')!;
    expect(agent.parameters.properties['task']?.maxLength).toBe(MAX_TASK_CHARS);
    expect(agent.parameters.properties).toHaveProperty('isolate');
    expect([...commands.keys()]).toEqual(['agents']);
    const notes: string[] = [];
    await commands.get('agents')!.handler('kill nobody-1', { ui: { notify: (text: string) => notes.push(text) } });
    expect(notes[0]).toBe('No subagent "nobody-1". None is running.');
    expect(subagentProcessEnv(undefined, {}, { id: 'a-1', workspace: '/repo' })['OCTOCODE_TEAM_WORKSPACE']).toBe('/repo');
  });

  it('sums child model usage', () => {
    const total = emptyUsage();
    addUsage(total, { input: 10, output: 5, cacheRead: 1, cacheWrite: 2, totalTokens: 18, cost: { input: 0.1, output: 0.2, cacheRead: 0, cacheWrite: 0, total: 0.3 } });
    addUsage(total, { output: 5, cost: { total: 0.1 } });
    addUsage(total, undefined);
    expect(total).toMatchObject({ input: 10, output: 10, totalTokens: 18 });
    expect(total.cost.total).toBeCloseTo(0.4);
  });

  it('extracts the final assistant text from a JSON event', () => {
    expect(assistantText({ role: 'assistant', content: [{ type: 'thinking', thinking: '…' }, { type: 'text', text: 'Answer' }] })).toBe('Answer');
    expect(assistantText({ role: 'user', content: [] })).toBeUndefined();
  });
});
