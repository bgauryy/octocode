import { describe, expect, it, vi } from 'vitest';
import { Permission } from '../src/permission/ask.js';
import type { Answer } from '../src/permission/dialog.js';
import type { PermissionRequest } from '../src/permission/request.js';
import { fakeCtx, fakePi } from './fake-pi.js';
import { tmp } from './helpers.js';

const request = (): PermissionRequest => ({ action: 'shell', title: 'Run?', what: ['npm test'], grant: { key: 'npm test', label: '`npm test`' } });
function setup(mode = 'manual') {
  const fake = fakePi();
  const permission = new Permission({ pi: fake.pi, env: { OCTOCODE_PERMISSION: mode }, isSubagent: false });
  const ctx = fakeCtx({ cwd: tmp(), hasUI: true });
  permission.start(ctx);
  return { permission, ctx };
}

describe('permission lifecycle', () => {
  it('uses a new scoped grant for an already queued call without asking twice', async () => {
    const { permission, ctx } = setup();
    let answer!: (value: Answer) => void;
    const dialog = vi.fn(async (): Promise<Answer> => dialog.mock.calls.length === 1 ? new Promise((resolve) => { answer = resolve; }) : undefined);
    const first = permission.check(ctx, request(), dialog);
    const second = permission.check(ctx, request(), dialog);
    await vi.waitFor(() => expect(dialog).toHaveBeenCalledTimes(1));
    answer('session');
    expect(await Promise.all([first, second])).toEqual([{ allowed: true, via: 'user', lasts: 'session' }, { allowed: true, via: 'grant', lasts: 'session' }]);
    expect(dialog).toHaveBeenCalledTimes(1);
  });

  it.each(['session', 'grants', 'mode'] as const)('rejects an old dialog answer after a %s change', async (change) => {
    const { permission, ctx } = setup();
    let answer!: (value: Answer) => void;
    const dialog = vi.fn(async () => new Promise<Answer>((resolve) => { answer = resolve; }));
    const pending = permission.check(ctx, request(), dialog);
    await vi.waitFor(() => expect(dialog).toHaveBeenCalledTimes(1));
    if (change === 'session') permission.start(ctx);
    else if (change === 'grants') permission.clearGrants();
    else permission.setMode('workspace', ctx);
    answer('kind');
    expect(await pending).toMatchObject({ allowed: false, via: 'cancelled' });
    expect(permission.grants()).toEqual([]);
  });

  it('does not authorize an already cancelled request through a mode or grant', async () => {
    const { permission, ctx } = setup('full');
    const signal = AbortSignal.abort();
    expect(await permission.check(ctx, { ...request(), signal })).toMatchObject({ allowed: false, via: 'cancelled' });
    permission.setMode('manual', ctx);
    await permission.check(ctx, request(), async () => 'session');
    expect(await permission.check(ctx, { ...request(), signal })).toMatchObject({ allowed: false, via: 'cancelled' });
  });

  it('drops broad grants when the user chooses a stricter mode', async () => {
    const { permission, ctx } = setup('workspace');
    await permission.check(ctx, { action: 'file.outside', title: 'Change?', what: ['/tmp/a'] }, async () => 'kind');
    permission.setMode('manual', ctx);
    expect(permission.grants()).toEqual([]);
    expect(await permission.check({ ...ctx, hasUI: false }, { action: 'file.outside', title: 'Change?', what: ['/tmp/b'] })).toMatchObject({ allowed: false, via: 'headless' });
  });

  it('treats Esc while entering denial feedback as cancellation', async () => {
    const { permission, ctx } = setup();
    ctx.ui.select = vi.fn(async () => 'Deny and tell the agent why…') as never;
    ctx.ui.input = vi.fn(async () => undefined) as never;
    expect(await permission.check(ctx, request())).toMatchObject({ allowed: false, via: 'cancelled' });
  });

  it('never grants destructive MCP actions for a session, including inherited grants', async () => {
    const { permission, ctx } = setup('workspace');
    const destructive: PermissionRequest = { action: 'mcp.destructive', title: 'Drop?', what: ['drop table'], grant: { key: 'drop', label: 'drop' } };
    const offered: string[][] = [];
    ctx.ui.select = vi.fn(async (_title: string, choices: string[]) => { offered.push(choices); return 'Allow once'; }) as never;
    await permission.check(ctx, destructive);
    expect(offered[0]).toEqual(['Allow once', 'Deny', 'Deny and tell the agent why…']);
    expect(await permission.check(ctx, destructive, async () => 'kind')).toMatchObject({ allowed: false });
    expect(await permission.check(ctx, destructive, async () => 'session')).toMatchObject({ allowed: false });
    const child = new Permission({ pi: fakePi().pi, env: { OCTOCODE_PERMISSION: 'workspace', OCTOCODE_PERMISSION_GRANTS: 'mcp.destructive' }, isSubagent: true });
    child.start(ctx);
    expect(await child.check(ctx, destructive)).toMatchObject({ allowed: false, via: 'headless' });
  });
});
