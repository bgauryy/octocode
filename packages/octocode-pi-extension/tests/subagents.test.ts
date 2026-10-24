import fs from 'node:fs';
import path from 'node:path';
import { parseArgs } from '@earendil-works/pi-coding-agent';
import { describe, expect, it } from 'vitest';
import { addUsage, emptyUsage } from '../src/shared/util.js';
import { assistantText } from '../src/subagents/events.js';
import { buildAgentArgs, subagentProcessEnv } from '../src/subagents/process.js';
import { processStartTime } from '../src/shared/process.js';
import { loadProfiles, parseProfile } from '../src/subagents/profiles.js';
import { Subcommands } from '../src/shared/commands.js';
import { maxSubagents, Slots, subagentLimitError, teamTask } from '../src/subagents/claim.js';
import { registerAgentTool } from '../src/subagents/tool.js';
import { octocodePrompt } from '../src/prompt.js';
import { Permission } from '../src/permission/ask.js';
import { askSpawn, profileReach } from '../src/subagents/profiles.js';
import { fakeCtx, fakePi, fakePermission } from './fake-pi.js';
import { tmp } from './helpers.js';

/** A child's saved session, as `childSession` names it. */
const SESSION = { dir: '/s/session', id: 'inky-kraken' };

describe('subagent permissions', () => {
  it('sets the child mode and grant from the parent, never from the inherited environment', () => {
    const env = subagentProcessEnv(undefined, { OCTOCODE_PERMISSION: 'full', OCTOCODE_PERMISSION_GRANTS: 'file.outside' }, { id: 'a-1', permission: { mode: 'manual', grants: ['shell'] } });
    expect(env).toMatchObject({ OCTOCODE_PERMISSION: 'manual', OCTOCODE_PERMISSION_GRANTS: 'shell' });
    const none = subagentProcessEnv(undefined, { OCTOCODE_PERMISSION: 'full', OCTOCODE_PERMISSION_GRANTS: 'shell' }, { id: 'a-1', permission: { mode: 'workspace', grants: [] } });
    expect(none['OCTOCODE_PERMISSION']).toBe('workspace');
    expect(none).not.toHaveProperty('OCTOCODE_PERMISSION_GRANTS');
  });

  it('grants only the workspace classes a profile\'s tools reach', () => {
    const profile = (fields: object) => ({ name: 'p', description: '', prompt: '', ...fields });
    expect(profileReach(undefined)).toEqual(['file.workspace', 'shell']);
    expect(profileReach(profile({ excludeTools: 'file' }))).toEqual(['shell']);
    expect(profileReach(profile({ tools: 'read,grep' }))).toEqual([]);
    expect(profileReach(profile({ tools: 'read,edit,bash', excludeTools: 'bash' }))).toEqual(['file.workspace']);
  });

  it('asks once in manual and passes the grant; a denied spawn throws before anything starts', async () => {
    const fake = fakePi();
    const permission = new Permission({ pi: fake.pi, env: { OCTOCODE_PERMISSION: 'manual' }, isSubagent: false });
    const ctx = fakeCtx({ cwd: tmp(), ui: { selects: ['Allow once', 'Deny'] } });
    permission.start(ctx);
    let title = '';
    const select = ctx.ui.select;
    ctx.ui.select = (async (text: string, choices: string[]) => ((title ||= text), select(text, choices))) as never;
    const implementer = { name: 'implementer', description: '', prompt: '' };
    expect(await askSpawn(permission, ctx, implementer, 'Fix the bug')).toEqual(['file.workspace', 'shell']);
    expect(title).toMatch(/implementer may edit files in the repository and run shell commands[\s\S]*Fix the bug/);
    await expect(askSpawn(permission, ctx, implementer, 'Again')).rejects.toThrow(/denied starting a subagent/);
    permission.setMode('workspace', ctx);
    // A mode that allows the spawn passes no grant: the child inherits the mode itself.
    expect(await askSpawn(permission, ctx, implementer, 'Fix')).toEqual([]);
  });
});

describe('profile permissions', () => {
  const text = (permissions: string) => `---\nname: web\ndescription: Browses\nexcludeTools: file,bash\npermissions: ${permissions}\n---\nBrowse.`;
  const spawnCtx = (selects: string[]) => {
    const ctx = fakeCtx({ cwd: tmp(), ui: { selects } });
    const titles: string[] = [];
    const select = ctx.ui.select;
    ctx.ui.select = (async (title: string, choices: string[]) => (titles.push(title), select(title, choices))) as never;
    return { ctx, titles };
  };

  it('reads only the declarable classes, reports the rest, and hashes the content', () => {
    const profile = parseProfile('x', text('[browser.control, shell, net.private, shell.destructive]'));
    expect(profile.permissions).toEqual(['browser.control', 'net.private']);
    expect(profile.ignoredPermissions).toEqual(['shell', 'shell.destructive']);
    expect(parseProfile('x', text('[browser.control]')).hash).not.toBe(parseProfile('x', text('[browser.control, net.private]')).hash);
    expect(parseProfile('x', '---\nname: plain\n---\nBody.').permissions).toBeUndefined();
  });

  it('forces one spawn prompt listing each class in workspace, remembers the answer, and asks again after an edit', async () => {
    const fake = fakePi();
    const permission = new Permission({ pi: fake.pi, env: { OCTOCODE_PERMISSION: 'workspace' }, isSubagent: false });
    const { ctx, titles } = spawnCtx(['Allow once', 'Allow once', 'Deny']);
    permission.start(ctx);
    const profile = parseProfile('x', text('[browser.control, net.private]'));
    expect(await askSpawn(permission, ctx, profile, 'look')).toEqual(['browser.control', 'net.private']);
    expect(titles).toHaveLength(1);
    expect(titles[0]).toContain('permission it declares: browser.control');
    expect(titles[0]).toContain('permission it declares: net.private');
    expect(titles[0]).toContain('▸ why it asks: the profile declares browser.control');
    // The same profile content is not asked about again; a changed one is.
    expect(await askSpawn(permission, ctx, profile, 'look again')).toEqual(['browser.control', 'net.private']);
    expect(titles).toHaveLength(1);
    const edited = parseProfile('x', text('[browser.control, net.private]').replace('Browse.', 'Browse more.'));
    expect(await askSpawn(permission, ctx, edited, 'look')).toEqual(['browser.control', 'net.private']);
    expect(titles).toHaveLength(2);
    permission.clearGrants();
    await expect(askSpawn(permission, ctx, profile, 'look')).rejects.toThrow(/denied starting a subagent/);
  });

  it('runs a declaring profile in full without a prompt, and still records the reason', async () => {
    const fake = fakePi();
    const permission = new Permission({ pi: fake.pi, env: { OCTOCODE_PERMISSION: 'full' }, isSubagent: false, stderr: () => undefined });
    const { ctx, titles } = spawnCtx([]);
    permission.start(ctx);
    expect(await askSpawn(permission, ctx, parseProfile('x', text('[browser.control]')), 'look')).toEqual(['browser.control']);
    expect(titles).toEqual([]);
    expect(fake.entries.map((entry) => entry.data)).toEqual([expect.objectContaining({ action: 'agent.spawn', reasons: ['profile.permissions'], via: 'mode' })]);
  });

  it('keeps evaluate for the web profiles by declaring browser.control', () => {
    for (const name of ['webHeadless', 'webLive']) {
      const profile = parseProfile(name, fs.readFileSync(path.join(__dirname, '..', 'subagents', `${name}.md`), 'utf8'));
      expect(profile.permissions, name).toEqual(['browser.control']);
      expect(profile.ignoredPermissions, name).toBeUndefined();
    }
  });
});

describe('team task', () => {
  it('appends one Teammates section that names the parent and lists each running teammate with its goal', () => {
    const [given, teammates, ...rest] = teamTask('Fix A', 'main-1', [['researcher-2', 'Trace B'], ['coder-3', 'Fix C']]).split('\n\n## Teammates\n');
    expect(given).toBe('Fix A');
    expect(rest).toEqual([]);
    expect(teammates).toContain('main-1');
    expect(teammates!.split('\n').filter((line) => line.startsWith('- '))).toEqual(['- `researcher-2`: Trace B', '- `coder-3`: Fix C']);
    expect(teamTask('Fix A', 'main-1', [])).toMatch(/none yet; `coordinate list` shows teammates that start later/);
  });
});

describe('subagents', () => {
  it('collaborate mode: flags the child and its prompt', () => {
    expect(subagentProcessEnv(undefined, {}, { id: 'a-1', collaborate: true })['OCTOCODE_AGENT_COLLABORATE']).toBe('1');
    // The parent names its session for backlog ownership; an inherited value from a grandparent never leaks through.
    expect(subagentProcessEnv(undefined, { OCTOCODE_PARENT_SESSION: 'old' }, { id: 'a-1', parentSession: 's-1' })['OCTOCODE_PARENT_SESSION']).toBe('s-1');
    expect(subagentProcessEnv(undefined, { OCTOCODE_PARENT_SESSION: 'old' }, { id: 'a-1' })).not.toHaveProperty('OCTOCODE_PARENT_SESSION');
    // The parent's pid and its start time: a later process that reuses the pid is not taken for the parent.
    const started = processStartTime(process.pid);
    expect(subagentProcessEnv(undefined, { OCTOCODE_PARENT_PID: '1' }, { id: 'a-1' })['OCTOCODE_PARENT_PID']).toBe(started ? `${process.pid} ${started}` : String(process.pid));
    // The user's command hooks stay in the main session unless the profile opts in.
    expect(subagentProcessEnv(undefined, { OCTOCODE_HOOKS: '1', PATH: '/bin' })).not.toHaveProperty('OCTOCODE_HOOKS');
    expect(subagentProcessEnv(undefined, { OCTOCODE_HOOKS: '1', PATH: '/bin' })['PATH']).toBe('/bin');
    expect(subagentProcessEnv({ name: 'h', description: '', prompt: '', hooks: true }, { OCTOCODE_HOOKS: '1' })['OCTOCODE_HOOKS']).toBe('1');
    expect(subagentProcessEnv({ name: 'h', description: '', prompt: '', hooks: true }, {})).not.toHaveProperty('OCTOCODE_HOOKS');
    expect(parseProfile('x', '---\nhooks: true\n---\nbody').hooks).toBe(true);
    // Switches take YAML booleans only; a quoted string is refused rather than read as a guess.
    expect(parseProfile('x', '---\nmcp: false\nvisibleBrowser: false\n---\nbody')).toMatchObject({ mcp: false });
    for (const value of ['"true"', '"false"', '1', 'yes']) expect(() => parseProfile('x', `---\nmcp: ${value}\n---\nbody`), value).toThrow(/mcp must be true or false/);
    expect(subagentProcessEnv(undefined, {}, { id: 'a-1' })['OCTOCODE_AGENT_COLLABORATE']).toBeUndefined();
    const child = (collaborate: boolean) => octocodePrompt({ octocode: false, profiles: [], canDelegate: false, collaborate });
    expect(child(true)).toMatch(/You are on a team/);
    expect(child(false)).not.toMatch(/You are on a team/);
  });

  it('parses profile frontmatter and builds child pi arguments', () => {
    const profile = parseProfile('fallback', '---\nname: researcher\ndescription: Reads code\ntools: read,grep\n---\nBe precise.');
    expect(profile).toEqual({ name: 'researcher', description: 'Reads code', prompt: 'Be precise.', tools: 'read,grep,sendMessage,coordinate,archive', hash: expect.stringMatching(/^[0-9a-f]{16}$/) });
    const dir = tmp();
    const args = buildAgentArgs(profile, 'anthropic/claude', '/ext/index.js', dir, SESSION);
    const promptFile = args[args.indexOf('--append-system-prompt') + 1]!;
    expect(args).toEqual([
      '--mode', 'rpc', '--session-dir', SESSION.dir, '--session-id', SESSION.id, '--no-extensions', '-e', '/ext/index.js', '-e', 'builtin:mcp', '-e', 'builtin:tool-search', '--model', 'anthropic/claude', '--tools', 'read,grep,sendMessage,coordinate,archive', '--append-system-prompt', promptFile,
    ]);
    expect(path.dirname(promptFile)).toBe(dir);
    expect(fs.readFileSync(promptFile, 'utf8')).toBe('Be precise.');
  });

  it('hands the profile prompt to Pi as an owner-only file, so a prompt that names a path is never read as that file', () => {
    const dir = tmp();
    const decoy = path.join(tmp(), 'secret.txt');
    fs.writeFileSync(decoy, 'not the prompt');
    const args = buildAgentArgs({ name: 'p', description: '', prompt: decoy }, undefined, '/ext/index.js', dir, SESSION);
    const promptFile = args[args.indexOf('--append-system-prompt') + 1]!;
    expect(promptFile).not.toBe(decoy);
    expect(fs.readFileSync(promptFile, 'utf8')).toBe(decoy);
    if (process.platform !== 'win32') expect(fs.statSync(promptFile).mode & 0o777).toBe(0o600);
    // No prompt, no file.
    expect(buildAgentArgs({ name: 'p', description: '', prompt: '' }, undefined, '/ext/index.js', dir, SESSION)).not.toContain('--append-system-prompt');
  });

  it('keeps the task out of the arguments: RPC takes it as a prompt command, so no task text can become a flag or an @file', () => {
    const profile = parseProfile('p', '---\ntools: read\n---\n- Be careful.');
    const parsed = parseArgs(buildAgentArgs(profile, 'faux/model', '/ext/index.js', tmp(), SESSION));
    expect(parsed.mode).toBe('rpc');
    expect(parsed.messages).toEqual([]);
    expect(parsed.fileArgs).toEqual([]);
    expect(parsed.model).toBe('faux/model');
    expect(parsed.extensions).toEqual(['/ext/index.js', 'builtin:mcp', 'builtin:tool-search']);
    expect(parsed.appendSystemPrompt?.map((file) => fs.readFileSync(file, 'utf8'))).toEqual(['- Be careful.']);
    expect(parsed.diagnostics).toEqual([]);
  });

  it('loads Pi\'s built-in MCP in children unless the profile turns MCP off', () => {
    const args = buildAgentArgs({ name: 'plain', description: '', prompt: '', mcp: false }, undefined, '/ext/index.js', undefined, SESSION);
    expect(args).not.toContain('builtin:mcp');
    expect(args).not.toContain('builtin:tool-search');
  });

  it('maps excludeTools frontmatter to --exclude-tools', () => {
    const profile = parseProfile('reviewer', '---\nexcludeTools: file\n---\nReview.');
    expect(profile.excludeTools).toBe('file');
    const args = buildAgentArgs(profile, undefined, '/ext/index.js', tmp(), SESSION);
    expect(args).toEqual([
      '--mode', 'rpc', '--session-dir', SESSION.dir, '--session-id', SESSION.id, '--no-extensions', '-e', '/ext/index.js', '-e', 'builtin:mcp', '-e', 'builtin:tool-search', '--exclude-tools', 'file,edit,write', '--append-system-prompt', args[args.indexOf('--append-system-prompt') + 1],
    ]);
  });

  it('loads project profiles only for trusted projects', () => {
    const cwd = tmp();
    fs.mkdirSync(path.join(cwd, '.pi', 'agents'), { recursive: true });
    fs.writeFileSync(path.join(cwd, '.pi', 'agents', 'local.md'), '---\ndescription: Project profile\n---\nDo it.');
    expect(loadProfiles(cwd, tmp(), false).has('local')).toBe(false);
    expect(loadProfiles(cwd, tmp(), true).has('local')).toBe(true);
  });

  it('skips a malformed or unreadable profile and still loads the rest, bundled ones included', () => {
    const cwd = tmp();
    const dir = path.join(cwd, '.pi', 'agents');
    fs.mkdirSync(path.join(dir, 'folder.md'), { recursive: true });
    fs.writeFileSync(path.join(dir, 'broken.md'), '---\nname: [a\n---\nbody');
    fs.writeFileSync(path.join(dir, 'good.md'), '---\ndescription: Fine\n---\nDo it.');
    const skipped: string[] = [];
    const profiles = loadProfiles(cwd, tmp(), true, {}, (file) => skipped.push(path.basename(file)));
    expect(profiles.has('good')).toBe(true);
    expect(profiles.has('reviewer')).toBe(true);
    expect(skipped.sort()).toEqual(['broken.md', 'folder.md']);
  });

  it('offers a headless web profile that never opens a visible browser', () => {
    const headless = loadProfiles(tmp(), tmp()).get('webHeadless');
    expect(headless?.excludeTools?.split(',')).toEqual(['file', 'bash']);
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
    expect(buildAgentArgs(profile, undefined, '/ext/index.js', undefined, SESSION)).toContain('--exclude-tools');
  });

  it('caps running subagents at OCTOCODE_MAX_SUBAGENTS, default 3', () => {
    expect(maxSubagents({})).toBe(3);
    expect(maxSubagents({ OCTOCODE_MAX_SUBAGENTS: '5' })).toBe(5);
    for (const bad of ['0', '-1', '2.5', 'many', '']) expect(maxSubagents({ OCTOCODE_MAX_SUBAGENTS: bad }), bad).toBe(3);
    const refusal = subagentLimitError(['a', 'b', 'c'], 3);
    expect(refusal).toContain('3 subagents are already running (a, b, c)');
    expect(refusal).toContain('OCTOCODE_MAX_SUBAGENTS');
  });

  it('makes a call past the limit wait for a slot, first come first served, and lets it give up', async () => {
    const slots = new Slots(() => 1);
    await slots.take(undefined);
    const waits: number[] = [];
    const order: string[] = [];
    const second = slots.take(undefined, (running) => waits.push(running)).then(() => order.push('second'));
    const cancelled = new AbortController();
    const third = slots.take(cancelled.signal).then(() => order.push('third'), (error: Error) => order.push(error.message));
    const fourth = slots.take(undefined).then(() => order.push('fourth'));
    expect(waits).toEqual([1]);
    expect(slots.tryTake()).toBe(false);
    cancelled.abort();
    await third;
    slots.give();
    await second;
    slots.give();
    await fourth;
    expect(order).toEqual(['Cancelled while waiting for a subagent slot.', 'second', 'fourth']);
    slots.give();
    expect(slots.tryTake()).toBe(true);
  });

  it('takes any task length, offers isolate, and registers /agents with kill and merge', async () => {
    const tools: Array<{ name: string; parameters: { properties: Record<string, { maxLength?: number; description?: string }> } }> = [];
    const commands = new Map<string, { handler: (args: string, ctx: unknown) => Promise<void> }>();
    const pi = { registerTool: (tool: never) => tools.push(tool), registerCommand: (name: string, command: never) => commands.set(name, command), registerMessageRenderer: () => undefined, on: () => undefined, sendMessage: () => undefined } as never;
    const control = registerAgentTool(pi, () => new Map(), { id: undefined, view: { members: [], traffic: [] }, members: () => [], setResumer: () => undefined } as never, fakePermission());
    // index.ts adds it as `/octocode agents` with the `/agents` shortcut.
    const subcommands = new Subcommands();
    subcommands.add('agents', control.command);
    subcommands.alias(pi, 'agents');
    const agent = tools.find((tool) => tool.name === 'agent')!;
    expect(agent.parameters.properties['task']?.maxLength).toBeUndefined();
    expect(agent.parameters.properties).toHaveProperty('isolate');
    // The parameter owns the collaborate guidance; the prompt does not repeat it.
    expect(agent.parameters.properties['collaborate']?.description).toMatch(/subagents that must coordinate directly/);
    expect(agent.parameters.properties['model']?.description).toMatch(/provider\/id, bare id or purpose \(small, medium\).*:thinking.*Refused when unavailable/);
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
