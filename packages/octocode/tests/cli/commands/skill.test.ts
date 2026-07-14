import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

// ─── Mocks ───────────────────────────────────────────────────────────────────

const executeDirectTool = vi.fn();

vi.mock('@octocodeai/octocode-tools-core/direct', () => ({
  executeDirectTool: (...args: unknown[]) => executeDirectTool(...args),
}));

vi.mock('@octocodeai/config', () => ({
  getOctocodeHome: () => '/mock-home/.octocode',
}));

vi.mock('node:os', () => ({
  homedir: () => '/mock-home',
}));

vi.mock('node:fs', () => ({
  existsSync: vi.fn().mockReturnValue(false),
  mkdirSync: vi.fn(),
  rmSync: vi.fn(),
  cpSync: vi.fn(),
  symlinkSync: vi.fn(),
  readdirSync: vi.fn().mockReturnValue([]),
}));

vi.mock('../../../src/utils/colors.js', () => ({
  c: (_color: string, s: string) => s,
  dim: (s: string) => s,
  bold: (s: string) => s,
}));

// ─── Imports (after mocks) ───────────────────────────────────────────────────

import * as nodefs from 'node:fs';
import { skillCommand } from '../../../src/cli/commands/skill.js';
import type { ParsedArgs } from '../../../src/cli/types.js';
import { EXIT } from '../../../src/cli/exit-codes.js';

// ─── Helpers ─────────────────────────────────────────────────────────────────

function run(
  args: string[] = [],
  options: Record<string, string | boolean> = {}
) {
  const parsed: ParsedArgs = { command: 'skill', args, options };
  return skillCommand.handler(parsed);
}

function cloneEnvelope(
  localPath = '/tmp/octocode/clone/skills/octocode-research'
) {
  return {
    isError: false,
    content: [],
    structuredContent: {
      results: [{ data: { localPath } }],
    },
  };
}

function listEnvelope(
  folders: string[] = ['octocode-research', 'octocode-rfc-generator']
) {
  return {
    isError: false,
    content: [],
    structuredContent: {
      results: [{ data: { folders } }],
    },
  };
}

function errorEnvelope(text = 'Not found') {
  return {
    isError: true,
    content: [{ type: 'text', text }],
    structuredContent: {},
  };
}

// ─── Tests ───────────────────────────────────────────────────────────────────

describe('skill command', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(nodefs.existsSync).mockReturnValue(false);
    executeDirectTool.mockResolvedValue(cloneEnvelope());
    process.exitCode = undefined;
    vi.spyOn(console, 'log').mockImplementation(() => undefined);
    vi.spyOn(console, 'error').mockImplementation(() => undefined);
  });

  afterEach(() => {
    vi.restoreAllMocks();
    process.exitCode = undefined;
  });

  // ── command structure ────────────────────────────────────────────────────

  it('has name "skill"', () => {
    expect(skillCommand.name).toBe('skill');
  });

  it('declares all required options', () => {
    const optNames = (skillCommand.options ?? []).map(o => o.name);
    const required = [
      'add',
      'name',
      'list',
      'platform',
      'target',
      'all',
      'mode',
      'force',
      'update',
      'dry-run',
      'verbose',
      'branch',
      'json',
      'install-all',
      'all-skills',
    ];
    for (const opt of required) {
      expect(optNames, `missing option --${opt}`).toContain(opt);
    }
  });

  // ── missing required option ──────────────────────────────────────────────

  it('exits USAGE when no source flag is provided', async () => {
    await run([], {});
    expect(process.exitCode).toBe(EXIT.USAGE);
  });

  it('outputs JSON error when no source flag and --json', async () => {
    await run([], { json: true });
    expect(process.exitCode).toBe(EXIT.USAGE);
    const logArg = (console.log as ReturnType<typeof vi.spyOn>).mock
      .calls[0][0] as string;
    const parsed = JSON.parse(logArg) as { success: boolean; error: string };
    expect(parsed.success).toBe(false);
    expect(typeof parsed.error).toBe('string');
  });

  // ── --list ────────────────────────────────────────────────────────────────

  it('--list calls ghViewRepoStructure and prints skills', async () => {
    executeDirectTool.mockResolvedValueOnce(listEnvelope());
    await run([], { list: true });
    expect(executeDirectTool).toHaveBeenCalledWith(
      'ghViewRepoStructure',
      expect.objectContaining({
        queries: expect.arrayContaining([
          expect.objectContaining({ owner: 'bgauryy', repo: 'octocode' }),
        ]),
      })
    );
    expect(process.exitCode).toBeUndefined(); // EXIT.OK
  });

  it('--list --json outputs JSON array of skills', async () => {
    executeDirectTool.mockResolvedValueOnce(listEnvelope(['a', 'b']));
    await run([], { list: true, json: true });
    const logArg = (console.log as ReturnType<typeof vi.spyOn>).mock
      .calls[0][0] as string;
    const parsed = JSON.parse(logArg) as { skills: string[] };
    expect(parsed.skills).toEqual(['a', 'b']);
  });

  it('--list exits RUNTIME on fetch error', async () => {
    executeDirectTool.mockRejectedValueOnce(new Error('network failure'));
    await run([], { list: true });
    expect(process.exitCode).toBe(EXIT.GENERAL);
  });

  // ── --name ─────────────────────────────────────────────────────────────────

  it('--name installs a named skill via ghCloneRepo then cpSync+symlinkSync', async () => {
    await run([], { name: 'octocode-research' });
    expect(executeDirectTool).toHaveBeenCalledWith(
      'ghCloneRepo',
      expect.objectContaining({
        queries: expect.arrayContaining([
          expect.objectContaining({
            owner: 'bgauryy',
            repo: 'octocode',
            sparsePath: 'skills/octocode-research',
          }),
        ]),
      })
    );
    // canonical copy
    expect(vi.mocked(nodefs.cpSync)).toHaveBeenCalled();
    // symlink to platform default (common)
    expect(vi.mocked(nodefs.symlinkSync)).toHaveBeenCalled();
    expect(process.exitCode).toBeUndefined();
  });

  it('--name + --json emits JSON result', async () => {
    await run([], { name: 'octocode-research', json: true });
    const logArg = (console.log as ReturnType<typeof vi.spyOn>).mock
      .calls[0][0] as string;
    const parsed = JSON.parse(logArg) as {
      skills: unknown[];
      platforms: string;
      mode: string;
      summary: unknown;
    };
    expect(parsed.skills).toHaveLength(1);
    expect(parsed.mode).toBe('symlink');
    expect(parsed.platforms).toBe('common');
  });

  it('--name + clone error sets exitCode RUNTIME', async () => {
    executeDirectTool.mockResolvedValueOnce(errorEnvelope('not found'));
    await run([], { name: 'unknown-skill' });
    expect(process.exitCode).toBe(EXIT.GENERAL);
  });

  // ── --dry-run ────────────────────────────────────────────────────────────

  it('--name --dry-run fetches from GitHub but skips cpSync and symlinkSync', async () => {
    await run([], { name: 'octocode-research', 'dry-run': true });
    // clone is still called to validate the path
    expect(executeDirectTool).toHaveBeenCalled();
    // but no actual file writes
    expect(vi.mocked(nodefs.cpSync)).not.toHaveBeenCalled();
    expect(vi.mocked(nodefs.symlinkSync)).not.toHaveBeenCalled();
    expect(process.exitCode).toBeUndefined();
  });

  it('--name --dry-run + canonical already exists skips fetch entirely', async () => {
    vi.mocked(nodefs.existsSync).mockReturnValue(true); // canonical already present
    await run([], { name: 'octocode-research', 'dry-run': true });
    expect(executeDirectTool).not.toHaveBeenCalled();
    expect(process.exitCode).toBeUndefined();
  });

  // ── --force / --update ────────────────────────────────────────────────────

  it('--force replaces existing destination', async () => {
    vi.mocked(nodefs.existsSync).mockReturnValue(true);
    await run([], { name: 'octocode-research', force: true });
    // canonical refreshed (rmSync + cpSync)
    expect(vi.mocked(nodefs.rmSync)).toHaveBeenCalled();
    expect(vi.mocked(nodefs.cpSync)).toHaveBeenCalled();
  });

  it('--update is an alias for --force', async () => {
    vi.mocked(nodefs.existsSync).mockReturnValue(true);
    await run([], { name: 'octocode-research', update: true });
    expect(vi.mocked(nodefs.rmSync)).toHaveBeenCalled();
  });

  // ── --platform ────────────────────────────────────────────────────────────

  it('--platform cursor installs to ~/.cursor/skills/<name>', async () => {
    await run([], { name: 'octocode-research', platform: 'cursor' });
    expect(vi.mocked(nodefs.symlinkSync)).toHaveBeenCalledWith(
      '/mock-home/.octocode/skills/octocode-research',
      '/mock-home/.cursor/skills/octocode-research',
      'junction'
    );
  });

  it('--all is shorthand for --platform all', async () => {
    await run([], { name: 'octocode-research', all: true });
    // Should create symlinks for multiple platforms
    expect(vi.mocked(nodefs.symlinkSync).mock.calls.length).toBeGreaterThan(1);
  });

  it('--target is an alias for --platform', async () => {
    await run([], { name: 'octocode-research', target: 'cursor' });
    expect(vi.mocked(nodefs.symlinkSync)).toHaveBeenCalledWith(
      expect.any(String),
      '/mock-home/.cursor/skills/octocode-research',
      'junction'
    );
  });

  // ── --mode ────────────────────────────────────────────────────────────────

  it('--mode copy uses cpSync for destinations instead of symlinkSync', async () => {
    await run([], { name: 'octocode-research', mode: 'copy' });
    // Should not call symlinkSync for any destination
    expect(vi.mocked(nodefs.symlinkSync)).not.toHaveBeenCalled();
    // Should call cpSync for canonical AND the platform destination
    expect(vi.mocked(nodefs.cpSync).mock.calls.length).toBeGreaterThanOrEqual(
      2
    );
  });

  it('--mode hybrid uses cpSync for claude and symlinkSync for others', async () => {
    await run([], {
      name: 'octocode-research',
      mode: 'hybrid',
      platform: 'cursor,claude',
    });
    // cursor → symlink, claude → copy
    const symlinkArgs = vi
      .mocked(nodefs.symlinkSync)
      .mock.calls.map((c: unknown[]) => c[1]) as string[];
    const copyArgs = vi
      .mocked(nodefs.cpSync)
      .mock.calls.map((c: unknown[]) => c[1]) as string[];
    expect(symlinkArgs.some((p: string) => p.includes('.cursor'))).toBe(true);
    expect(copyArgs.some((p: string) => p.includes('.claude'))).toBe(true);
  });

  it('invalid --mode exits USAGE', async () => {
    await run([], { name: 'octocode-research', mode: 'invalid' });
    expect(process.exitCode).toBe(EXIT.USAGE);
  });

  // ── --add ─────────────────────────────────────────────────────────────────

  it('--add installs from a custom GitHub path', async () => {
    await run([], { add: 'myorg/myrepo/skills/code-review' });
    expect(executeDirectTool).toHaveBeenCalledWith(
      'ghCloneRepo',
      expect.objectContaining({
        queries: expect.arrayContaining([
          expect.objectContaining({
            owner: 'myorg',
            repo: 'myrepo',
            sparsePath: 'skills/code-review',
          }),
        ]),
      })
    );
  });

  it('--add with full github.com URL parses correctly', async () => {
    await run([], {
      add: 'https://github.com/owner/repo/tree/main/skills/my-skill',
    });
    expect(executeDirectTool).toHaveBeenCalledWith(
      'ghCloneRepo',
      expect.objectContaining({
        queries: expect.arrayContaining([
          expect.objectContaining({
            owner: 'owner',
            repo: 'repo',
            sparsePath: 'skills/my-skill',
          }),
        ]),
      })
    );
  });

  it('--add with malformed path exits USAGE', async () => {
    await run([], { add: 'not-a-valid-path' });
    expect(process.exitCode).toBe(EXIT.USAGE);
  });

  // ── --install-all ─────────────────────────────────────────────────────────

  it('--install-all fetches skill list then installs each', async () => {
    executeDirectTool
      .mockResolvedValueOnce(listEnvelope(['skill-a', 'skill-b']))
      .mockResolvedValue(cloneEnvelope());
    await run([], { 'install-all': true });
    // first call = list, then one clone per skill
    expect(executeDirectTool).toHaveBeenCalledTimes(3);
    expect(process.exitCode).toBeUndefined();
  });

  it('--all-skills is an alias for --install-all', async () => {
    executeDirectTool
      .mockResolvedValueOnce(listEnvelope(['skill-a']))
      .mockResolvedValue(cloneEnvelope());
    await run([], { 'all-skills': true });
    expect(executeDirectTool).toHaveBeenCalledTimes(2);
  });

  it('--install-all exits RUNTIME when list fetch fails', async () => {
    executeDirectTool.mockRejectedValueOnce(new Error('timeout'));
    await run([], { 'install-all': true });
    expect(process.exitCode).toBe(EXIT.GENERAL);
  });

  // ── skip when destination already exists ────────────────────────────────

  it('skips existing destination without --force', async () => {
    vi.mocked(nodefs.existsSync)
      .mockReturnValueOnce(false) // canonical doesn't exist → refresh
      .mockReturnValue(true); // destination exists → skip
    await run([], { name: 'octocode-research', platform: 'cursor' });
    // No symlink/copy created for destination
    expect(vi.mocked(nodefs.symlinkSync)).not.toHaveBeenCalled();
  });
});
