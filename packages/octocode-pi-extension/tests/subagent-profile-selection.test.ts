import fs from 'node:fs';
import path from 'node:path';
import { createAgentSession, DefaultResourceLoader, parseArgs, SessionManager, SettingsManager } from '@earendil-works/pi-coding-agent';
import { describe, expect, it, vi } from 'vitest';
import octocode from '../src/index.js';
import { Permission } from '../src/permission/ask.js';
import { sessionOption } from '../src/permission/dialog.js';
import { buildAgentArgs } from '../src/subagents/process.js';
import { askSpawn, parseProfile, profileReach, type AgentProfile } from '../src/subagents/profiles.js';
import { fakeCtx, fakePermission, fakePi } from './fake-pi.js';
import { tmp } from './helpers.js';

describe('profile selectors against native Pi tool exposure', () => {
  const cases: Array<{ name: string; fields: Partial<AgentProfile>; defaults?: string[] }> = [
    { name: 'default tools', fields: {} },
    { name: 'all tools wildcard', fields: { tools: '*' } },
    { name: 'shell suffix wildcard', fields: { tools: '*sh' } },
    { name: 'native file tool wildcard', fields: { tools: 'w*' } },
    { name: 'read only allowlist', fields: { tools: 'read,grep' } },
    { name: 'empty allowlist', fields: { tools: ', ' } },
    { name: 'empty relative removal', fields: { tools: '-' } },
    { name: 'wildcard exclusions', fields: { excludeTools: 'f*,*sh' } },
    { name: 'extension tools with relative selectors', fields: { tools: '-file,-bash' } },
    { name: 'added native shell', fields: { tools: '+powershell', excludeTools: 'file,bash' } },
    { name: 'removed configured native shell', fields: { tools: '-powershell', excludeTools: 'file,bash' }, defaults: ['read', 'powershell'] },
    { name: 'configured native shell', fields: { excludeTools: 'file,bash' }, defaults: ['powershell'] },
    { name: 'ordered modifiers', fields: { tools: '+powershell,-powershell,+powershell', excludeTools: 'file,bash' } },
    { name: 'all excluded', fields: { tools: '*', excludeTools: '*' } },
  ];

  it.each(cases)('$name', async ({ fields, defaults }) => {
    const cwd = tmp();
    const agentDir = path.join(cwd, 'agent');
    fs.mkdirSync(agentDir);
    vi.stubEnv('OCTOCODE_SUBAGENT', '1');
    vi.stubEnv('OCTOCODE_MCP', '0');
    const profile = { name: 'selection-test', description: '', prompt: '', ...fields };
    const args = parseArgs(buildAgentArgs(profile, undefined, '/extension', undefined, { dir: path.join(cwd, 'session'), id: 'selection-test' }));
    const settingsManager = SettingsManager.inMemory({ compaction: { enabled: false }, retry: { enabled: false }, ...(defaults ? { defaultTools: defaults } : {}) });
    const resourceLoader = new DefaultResourceLoader({
      cwd, agentDir, settingsManager, extensionFactories: [{ name: 'octocode', factory: octocode }],
      noExtensions: true, noSkills: true, noPromptTemplates: true, noThemes: true, noContextFiles: true,
    });
    let session: Awaited<ReturnType<typeof createAgentSession>>['session'] | undefined;
    try {
      await resourceLoader.reload();
      ({ session } = await createAgentSession({ cwd, agentDir, resourceLoader, settingsManager, sessionManager: SessionManager.inMemory(cwd), ...(args.tools ? { tools: args.tools } : {}), ...(args.excludeTools ? { excludeTools: args.excludeTools } : {}) }));
      const active = new Set(session.getActiveToolNames());
      const nativeReach = [
        ...(['file', 'edit', 'write'].some((name) => active.has(name)) ? ['file.workspace'] : []),
        ...(['bash', 'powershell'].some((name) => active.has(name)) ? ['shell'] : []),
      ];
      expect(profileReach(profile, defaults)).toEqual(nativeReach);
    } finally {
      session?.dispose();
      vi.unstubAllEnvs();
    }
  });

  it.each(['read,+bash', '+ba*', '-ba*', 'read,+'])('rejects invalid native selectors before asking: %s', async (tools) => {
    const parsed = parseArgs(['--tools', tools]);
    const nativeError = parsed.diagnostics.find((diagnostic) => diagnostic.type === 'error')!.message;
    expect(() => parseProfile('invalid', `---\ntools: ${JSON.stringify(tools)}\n---\nTask`)).toThrow(nativeError);
    const permission = fakePermission();
    const check = vi.spyOn(permission, 'check');
    await expect(askSpawn(permission, fakeCtx({ cwd: tmp() }), { name: 'invalid', description: '', prompt: '', tools }, 'task')).rejects.toThrow(nativeError);
    expect(check).not.toHaveBeenCalled();
  });
});

describe('the spawn grant for a session', () => {
  const text = (body: string, front = '') => `---\nname: worker\ndescription: Works\n${front}---\n${body}`;

  it('is keyed by the profile\'s content: an edit under the same name asks again, the same content does not', async () => {
    const permission = new Permission({ pi: fakePi().pi, env: { OCTOCODE_PERMISSION: 'manual' }, isSubagent: false });
    const ctx = fakeCtx({ cwd: tmp() });
    permission.start(ctx);
    const grant = sessionOption('worker subagents');
    const asked: string[] = [];
    const replies = [grant, 'Deny', 'Deny', 'Deny', 'Deny'];
    ctx.ui.select = (async (title: string) => (asked.push(title), replies.shift())) as never;
    const profile = parseProfile('worker', text('Fix it.'));
    await askSpawn(permission, ctx, profile, 'one');
    expect(asked).toHaveLength(1);
    // Allowed for the session: the same content passes without a dialog.
    await askSpawn(permission, ctx, parseProfile('worker', text('Fix it.')), 'two');
    expect(asked).toHaveLength(1);
    // Any edit under the same name (prompt, model, tools, flags) is a new question; here each is refused.
    for (const edited of [text('Fix it, then deploy.'), text('Fix it.', 'model: small\n'), text('Fix it.', 'tools: read,bash\n'), text('Fix it.', 'hooks: true\n')]) {
      await expect(askSpawn(permission, ctx, parseProfile('worker', edited), 'three')).rejects.toThrow(/denied starting a subagent/);
    }
    expect(asked).toHaveLength(5);
  });
});
