import fs from 'node:fs';
import path from 'node:path';
import { describe, expect, it, vi } from 'vitest';
import { Permission } from '../src/permission/ask.js';
import { NEEDS_APPROVAL } from '../src/permission/dialog.js';
import { runSubagent } from '../src/subagents/process.js';
import { doneSummary } from '../src/subagents/render.js';
import { registerAgentTool } from '../src/subagents/tool.js';
import { Team } from '../src/team/session.js';
import { fakeCtx, fakePermission, fakePi } from './fake-pi.js';
import { git, tmp } from './helpers.js';
import { useScriptedPi } from './subagent-tool-harness.js';


useScriptedPi({ OCTOCODE_FAKE_DENIAL: `${NEEDS_APPROVAL} running a shell command (npm test) needs the user's approval (manual mode) and no one can be asked here.` });

describe("a child's refused actions", () => {
  it('are reported to the parent one by one, apart from ordinary tool failures', async () => {
    const denials: boolean[] = [];
    await runSubagent('denied', [], tmp(), undefined, { id: 'general-000001' }, undefined, ({ denied }) => {
      if (denied) denials.push(true);
    });
    expect(denials).toHaveLength(2);
  });

  it("show on the parent's agent row and in its result", async () => {
    const f = fakePi();
    const team = new Team(f.pi);
    registerAgentTool(f.pi, () => new Map(), team, fakePermission());
    const ctx = fakeCtx({ cwd: tmp() });
    Object.assign(ctx, { model: { provider: 'faux', id: 'model' }, modelRegistry: { getAvailable: () => [], find: () => undefined } });
    team.start(ctx);
    const result = await f.tools.get('agent').execute('call-1', { task: 'denied', goal: 'try denied things' }, undefined, undefined, ctx);
    expect(result.details.denied).toBe(2);
    // The child's report starts with `blocked`: the row says so.
    expect(result.details.outcome).toBe('blocked');
    expect(doneSummary(result.details, 1_000)).toMatch(/^Blocked \(/);
    expect(doneSummary(result.details, 1_000)).toContain('2 denied');
    expect(doneSummary({ ...result.details, denied: undefined }, 1_000)).not.toContain('denied');
    team.stop();
  }, 20_000);

  it("name the child's outcome in a background report's header and row", async () => {
    const f = fakePi();
    const team = new Team(f.pi);
    registerAgentTool(f.pi, () => new Map(), team, fakePermission());
    const ctx = fakeCtx({ cwd: tmp() });
    Object.assign(ctx, { model: { provider: 'faux', id: 'model' }, modelRegistry: { getAvailable: () => [], find: () => undefined } });
    team.start(ctx);
    await f.tools.get('agent').execute('call-1', { task: 'denied', goal: 'try denied things', background: true }, undefined, undefined, ctx);
    await vi.waitFor(() => expect(f.sent.length).toBeGreaterThan(0), { timeout: 10_000 });
    expect(String(f.sent[0]!.message.content)).toMatch(/^Background subagent [a-z]+-[a-z]+ \(general: try denied things\) reported blocked \(/);
    expect(f.sent[0]!.message.details).toMatchObject({ status: 'done', outcome: 'blocked', summary: expect.stringMatching(/^Blocked \(/) });
    team.stop();
  }, 20_000);
});

describe('kinds allowed for the session', () => {
  it('reach a child, which cannot ask anyone', async () => {
    const f = fakePi();
    const team = new Team(f.pi);
    const permission = new Permission({ pi: f.pi, env: { OCTOCODE_PERMISSION: 'manual' }, isSubagent: false });
    registerAgentTool(f.pi, () => new Map(), team, permission);
    // Untrusted, so manual: the web ask takes "all web requests", the spawn ask is allowed once.
    const ctx = Object.assign(fakeCtx({ cwd: tmp(), ui: { selects: ['Allow all web requests for this session', 'Allow once'] } }), { isProjectTrusted: () => false });
    Object.assign(ctx, { model: { provider: 'faux', id: 'model' }, modelRegistry: { getAvailable: () => [], find: () => undefined } });
    permission.start(ctx as never);
    team.start(ctx);
    expect(permission.mode).toBe('manual');
    await permission.check(ctx, { action: 'web', title: 'Fetch?', what: ['https://example.com'] });
    const result = await f.tools.get('agent').execute('call-1', { task: 'grants', goal: 'report its grants' }, undefined, undefined, ctx);
    const text = result.content[0].text as string;
    expect(text).toContain('mode=manual');
    expect(/grants=(\S*)/.exec(text)?.[1]?.split(',')).toContain('web');
    team.stop();
  }, 20_000);
});

describe('an isolated child in a worktree of its parent\'s repository', () => {
  function worktree(): { repo: string; tree: string } {
    const repo = fs.realpathSync(tmp('octocode-perm-repo-'));
    git(repo, 'init', '-q', '-b', 'main');
    git(repo, 'config', 'user.name', 'Test');
    git(repo, 'config', 'user.email', 'test@example.com');
    fs.writeFileSync(path.join(repo, 'a.txt'), 'one\n');
    git(repo, 'add', '-A');
    git(repo, 'commit', '-q', '-m', 'init');
    const tree = path.join(fs.realpathSync(tmp('octocode-perm-trees-')), 'child');
    git(repo, 'worktree', 'add', '-q', tree);
    return { repo, tree };
  }
  const start = (env: NodeJS.ProcessEnv, cwd: string) => {
    const permission = new Permission({ pi: fakePi().pi, env, isSubagent: true });
    // Pi has never seen the worktree path: it does not trust it.
    permission.start({ cwd, isProjectTrusted: () => false, hasUI: false } as never);
    return permission.mode;
  };

  it("keeps the parent's full mode: the parent's trust covers its own repository's worktree", () => {
    const { repo, tree } = worktree();
    expect(start({ OCTOCODE_SUBAGENT: '1', OCTOCODE_TRUST_ROOT: repo, OCTOCODE_PERMISSION: 'full' }, tree)).toBe('full');
  });

  it('runs in the mode its parent set, wherever it works; without one it starts in the default full', () => {
    const { tree } = worktree();
    expect(start({ OCTOCODE_SUBAGENT: '1', OCTOCODE_PERMISSION: 'workspace' }, tree)).toBe('workspace');
    expect(start({ OCTOCODE_SUBAGENT: '1' }, tree)).toBe('full');
  });
});
