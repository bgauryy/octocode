import fs from 'node:fs';
import path from 'node:path';
import { createAgentSession, DefaultResourceLoader, parseArgs, SessionManager, SettingsManager } from '@earendil-works/pi-coding-agent';
import { describe, expect, it, vi } from 'vitest';
import octocode from '../src/index.js';
import { buildAgentArgs } from '../src/subagents/process.js';
import { CORE_TOOLS, loadProfiles, parseProfile } from '../src/subagents/profiles.js';
import { tmp } from './helpers.js';

const SESSION = { dir: '/tmp/session', id: 'core-test' };
const profile = (frontmatter: string) => parseProfile('core', `---\n${frontmatter}\n---\nTask`);
const allowed = (tools: string | undefined) => (tools ?? '').split(',');

describe('core tools of a subagent profile', () => {
  it('adds sendMessage, coordinate, read and archive to an allowlist, once, and passes them as --tools', () => {
    const parsed = profile('tools: edit,bash');
    expect(CORE_TOOLS.length).toBeGreaterThan(0);
    expect(allowed(parsed.tools)).toEqual(['edit', 'bash', ...CORE_TOOLS]);
    const args = parseArgs(buildAgentArgs(parsed, undefined, '/ext/index.js', tmp(), SESSION));
    expect(args.tools).toEqual(['edit', 'bash', ...CORE_TOOLS]);
    // A name already listed is not repeated.
    expect(allowed(profile('tools: read,grep').tools)).toEqual(['read', 'grep', ...CORE_TOOLS.filter((name) => name !== 'read')]);
  });

  it('counts a pattern that already matches a core tool, and adds the rest', () => {
    expect(profile('tools: "*"').tools).toBe('*');
    expect(allowed(profile('tools: "send*,edit"').tools)).toEqual(['send*', 'edit', ...CORE_TOOLS.filter((name) => !name.startsWith('send'))]);
  });

  it('gives an empty allowlist the core tools: it still reaches its parent', () => {
    expect(allowed(profile('tools: ","').tools)).toEqual([...CORE_TOOLS]);
  });

  it('leaves a profile without tools alone: Pi\'s default selection already has them and --tools would replace it', () => {
    const parsed = profile('excludeTools: file,browser');
    expect(parsed.tools).toBeUndefined();
    expect(buildAgentArgs(parsed, undefined, '/ext/index.js', tmp(), SESSION)).not.toContain('--tools');
  });

  it('leaves a relative list alone: it changes the default selection, which has the core tools', () => {
    expect(profile('tools: "+powershell,-bash"').tools).toBe('+powershell,-bash');
  });

  it.each([
    ['excludeTools: sendMessage', ['sendMessage']],
    ['excludeTools: "coordinate,browser"', ['coordinate']],
    ['excludeTools: read', ['read']],
    ['excludeTools: "*"', [...CORE_TOOLS]],
    ['excludeTools: "send*"', ['sendMessage']],
    ['tools: "-read"', ['read']],
    ['tools: "edit,bash"\nexcludeTools: coordinate', ['coordinate']],
  ])('refuses %j when it loads, naming the profile and the tools', (frontmatter, names) => {
    expect(() => profile(frontmatter)).toThrow(`Profile "core" removes ${names.join(', ')} with excludeTools or a "-" entry in tools`);
    expect(() => profile(frontmatter)).toThrow(/Remove (it|them) from the profile's excludeTools/);
  });

  it('reports a refused profile through onSkip and keeps loading the others', () => {
    const cwd = tmp();
    const dir = path.join(cwd, '.pi', 'agents');
    fs.mkdirSync(dir, { recursive: true });
    fs.writeFileSync(path.join(dir, 'blind.md'), '---\nexcludeTools: sendMessage\n---\nDo it.');
    fs.writeFileSync(path.join(dir, 'fine.md'), '---\ntools: edit\n---\nDo it.');
    const skipped: string[] = [];
    const profiles = loadProfiles(cwd, tmp(), true, {}, (file, error) => skipped.push(`${path.basename(file)}: ${(error as Error).message}`));
    expect(profiles.has('blind')).toBe(false);
    expect(allowed(profiles.get('fine')?.tools)).toEqual(['edit', 'read', 'sendMessage', 'coordinate', 'archive']);
    expect(skipped).toHaveLength(1);
    expect(skipped[0]).toMatch(/^blind\.md: Profile "blind" removes sendMessage/);
    // The bundled profiles all still load.
    for (const name of ['researcher', 'reviewer', 'implementer', 'webHeadless', 'webLive']) expect(profiles.has(name)).toBe(true);
  });

  it('keeps the core tools active in a real Pi session started with the profile\'s allowlist, and drops them without the addition', async () => {
    const cwd = tmp();
    const agentDir = path.join(cwd, 'agent');
    fs.mkdirSync(agentDir);
    vi.stubEnv('OCTOCODE_SUBAGENT', '1');
    vi.stubEnv('OCTOCODE_MCP', '0');
    const activeTools = async (tools: string | undefined) => {
      const settingsManager = SettingsManager.inMemory({ compaction: { enabled: false }, retry: { enabled: false } });
      const resourceLoader = new DefaultResourceLoader({
        cwd, agentDir, settingsManager, extensionFactories: [{ name: 'octocode', factory: octocode }],
        noExtensions: true, noSkills: true, noPromptTemplates: true, noThemes: true, noContextFiles: true,
      });
      await resourceLoader.reload();
      const { session } = await createAgentSession({ cwd, agentDir, resourceLoader, settingsManager, sessionManager: SessionManager.inMemory(cwd), ...(tools ? { tools: parseArgs(['--tools', tools]).tools! } : {}) });
      try {
        return session.getActiveToolNames();
      } finally {
        session.dispose();
      }
    };
    try {
      expect(await activeTools(profile('tools: edit').tools)).toEqual(expect.arrayContaining(['edit', ...CORE_TOOLS]));
      // The evidence for the addition: the bare allowlist loses the messaging tools.
      expect(await activeTools('edit')).not.toContain('sendMessage');
    } finally {
      vi.unstubAllEnvs();
    }
  });
});
