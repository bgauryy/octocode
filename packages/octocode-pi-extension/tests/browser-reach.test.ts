import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { describe, expect, it, vi } from 'vitest';
import { checkBrowserUrl, checkUploadPaths } from '../src/browser/access.js';
import { BrowserTool, registerBrowserTool } from '../src/browser/tool.js';
import { fakeCtx, fakePermission, fakePi } from './fake-pi.js';

describe('browser reach', () => {
  /** A UI whose permission dialog answers `answer` (the select's choice); true allows once, false denies. */
  const ui = (answer: boolean | string) => {
    const select = vi.fn(async (_title: string, _choices: string[]) => (answer === true ? 'Allow once' : answer === false ? 'Deny' : answer));
    return { hasUI: true, cwd: '/work', ui: { select } as never, select };
  };

  it('opens http(s) only, and private or local hosts only once the user allows them', async () => {
    const allowed = fakePermission('workspace');
    await expect(checkBrowserUrl('file:///etc/passwd', ui(true), allowed, { toolCallId: 'call' })).rejects.toThrow(/http\(s\) URLs only/);
    await expect(checkBrowserUrl('javascript:alert(1)', ui(true), allowed, { toolCallId: 'call' })).rejects.toThrow(/http\(s\) URLs only/);
    await expect(checkBrowserUrl('not a url', ui(true), allowed, { toolCallId: 'call' })).rejects.toThrow(/Not a URL/);
    await expect(checkBrowserUrl('https://93.184.216.34/', { hasUI: false, ui: {} as never }, allowed, { toolCallId: 'call' })).resolves.toEqual({ host: { kind: 'public', addresses: ['93.184.216.34'] } });
    await expect(checkBrowserUrl('http://169.254.169.254/latest/meta-data', { hasUI: false, ui: {} as never }, allowed, { toolCallId: 'call' })).rejects.toThrow(/Blocked private or local address/);
    const declined = ui(false);
    await expect(checkBrowserUrl('http://127.0.0.1:3000/', declined, allowed, { toolCallId: 'call' })).rejects.toThrow(/Blocked private or local address: 127\.0\.0\.1:3000\. The user denied opening a private or local address.*Do not retry it/);
    // The session grant is per host: the second load of that host opens no dialog.
    const confirmed = ui('Allow 127.0.0.1:3000 for this session');
    await expect(checkBrowserUrl('http://127.0.0.1:3000/', confirmed, allowed, { toolCallId: 'call' })).resolves.toMatchObject({ privateGrant: 'session' });
    await checkBrowserUrl('http://127.0.0.1:3000/next', confirmed, allowed, { toolCallId: 'call' });
    expect(confirmed.select).toHaveBeenCalledTimes(1);
    expect(allowed.grants()).toEqual(['net.private: 127.0.0.1:3000']);
    // full opens private hosts without asking; manual asks like workspace.
    await expect(checkBrowserUrl('http://10.0.0.2/', { hasUI: false, ui: {} as never }, fakePermission('full'), { toolCallId: 'call' })).resolves.toEqual({ host: { kind: 'private' }, privateGrant: 'call' });
    await expect(checkBrowserUrl('http://10.0.0.2/', { hasUI: false, ui: {} as never }, fakePermission('manual'), { toolCallId: 'call' })).rejects.toThrow(/no one can be asked here/);
    // A name that does not resolve here may resolve to a private address in Chrome: it needs confirmation too.
    await expect(checkBrowserUrl('https://octocode.invalid/', { hasUI: false, ui: {} as never }, allowed, { toolCallId: 'call' })).rejects.toThrow(/Blocked unresolved host: octocode\.invalid/);
    const unresolved = ui(true);
    await expect(checkBrowserUrl('https://octocode.invalid/', unresolved, allowed, { toolCallId: 'call' })).resolves.toMatchObject({ host: { kind: 'unresolved' }, privateGrant: 'call' });
    expect(unresolved.select.mock.calls[0]?.[0]).toMatch(/^Open a private address\?\n {2}The browser wants to open octocode\.invalid, a name that does not resolve here and may reach a local or private address\./);
  });

  it('classifies the host once: a name that flips from public to private afterwards is a public page with no net.private grant', async () => {
    let answers = 0;
    const flipping = vi.fn(async () => [{ address: answers++ === 0 ? '93.184.216.34' : '10.0.0.5' }]);
    const permission = fakePermission('workspace');
    const dialog = ui(true);
    const checked = await checkBrowserUrl('http://rebind.example/', dialog, permission, { toolCallId: 'call', lookup: flipping });
    expect(checked).toEqual({ host: { kind: 'public', addresses: ['93.184.216.34'] } });
    expect(flipping).toHaveBeenCalledTimes(1);
    expect(dialog.select).not.toHaveBeenCalled();
    expect(permission.grants()).toEqual([]);
  });

  it('asks once, with both reasons, when the agent opens a private page in manual mode', async () => {
    const manual = ui(true);
    await checkBrowserUrl('http://127.0.0.1:3000/', manual, fakePermission('manual'), { toolCallId: 'call-1' });
    expect(manual.select).toHaveBeenCalledTimes(1);
    const shown = manual.select.mock.calls[0]![0];
    expect(shown.split('\n').filter((line) => line.includes('▸ why it asks:'))).toEqual([
      '  ▸ why it asks: the address is private or local, so it can reach internal services (manual mode asks)',
      '  ▸ why it asks: it acts in a real browser',
    ]);
    // workspace allows the browser action itself: the private address is the only reason.
    const workspace = ui(true);
    await checkBrowserUrl('http://127.0.0.1:3000/', workspace, fakePermission('workspace'), { toolCallId: 'call-2' });
    expect(workspace.select.mock.calls[0]![0].split('\n').filter((line) => line.includes('▸ why it asks:'))).toHaveLength(1);
    // A public page opens unasked in workspace.
    const open = ui(true);
    await checkBrowserUrl('https://93.184.216.34/', open, fakePermission('workspace'), { toolCallId: 'call-3' });
    expect(open.select).not.toHaveBeenCalled();
  });

  it('asks for every upload, and one prompt names an upload from outside the workspace too', async () => {
    const permission = fakePermission('workspace');
    const inside = ui(true);
    await expect(checkUploadPaths(['a.txt', '/work/sub/b.txt'], inside, permission)).resolves.toEqual(['/work/a.txt', '/work/sub/b.txt']);
    expect(inside.select.mock.calls[0]![0]).toMatch(/^Upload files to the page\?/);
    expect(inside.select.mock.calls[0]![1]).toEqual(['Allow once', 'Deny', 'Deny and tell the agent why…']);
    await expect(checkUploadPaths(['a.txt'], ui(false), permission)).rejects.toThrow(/^Refused to upload files: The user denied/);
    const outside = ui(true);
    await expect(checkUploadPaths(['../secret', '/etc/passwd'], outside, permission)).resolves.toHaveLength(2);
    expect(outside.select).toHaveBeenCalledTimes(1);
    const shown = outside.select.mock.calls[0]![0];
    expect(shown).toMatch(/^Upload files from outside the workspace\?/);
    expect(shown.split('\n').filter((line) => line.includes('▸ why it asks:'))).toHaveLength(2);
    await expect(checkUploadPaths(['../secret', '/etc/passwd'], ui(false), permission)).rejects.toThrow(`Refused to upload files outside the workspace: /secret, ${fs.realpathSync('/etc/passwd')}. The user denied`);
    await expect(checkUploadPaths(['/etc/passwd'], { cwd: '/work', hasUI: false, ui: {} as never }, permission)).rejects.toThrow(/outside the workspace.*no one can be asked here/);
    // full uploads without asking, and the audit says so.
    await expect(checkUploadPaths(['/etc/passwd'], { cwd: '/work', hasUI: false, ui: {} as never }, fakePermission('full'))).resolves.toHaveLength(1);
  });

  it.each([
    { action: 'navigate', url: 'http://127.0.0.1:3000/' },
    { action: 'upload', ref: 1, files: ['/private/tmp/outside-upload'] },
  ])('cancels an active $action approval with the tool request before page operations', async (params) => {
    const permission = fakePermission('workspace');
    const browser = new BrowserTool(permission);
    const page = { send: vi.fn() };
    vi.spyOn(browser, 'session').mockResolvedValue({ page } as never);
    const fake = fakePi();
    registerBrowserTool(fake.pi, browser);
    const ctx = fakeCtx({ cwd: '/private/tmp/browser-workspace' });
    const turn = new AbortController();
    let approvalSignal: AbortSignal | undefined;
    let dismiss: (answer: string | undefined) => void = () => undefined;
    let opened: () => void = () => undefined;
    const visible = new Promise<void>((resolve) => { opened = resolve; });
    ctx.ui.select = async (_title, _choices, options) => new Promise<string | undefined>((resolve) => {
      approvalSignal = options?.signal;
      dismiss = resolve;
      approvalSignal?.addEventListener('abort', () => resolve(undefined), { once: true });
      opened();
    });
    const pending = fake.tools.get('browser').execute('cancel-browser', params, turn.signal, undefined, ctx);
    const rejected = expect(pending).rejects.toThrow(/dismissed/);
    await visible;
    turn.abort();
    try {
      expect(approvalSignal?.aborted).toBe(true);
    } finally {
      dismiss(undefined);
      await rejected;
    }
    expect(page.send).not.toHaveBeenCalled();
    expect(permission.grants()).toEqual([]);
    await expect(checkBrowserUrl('http://127.0.0.1:3000/', ui(true), permission, { toolCallId: 'call' })).resolves.toMatchObject({ privateGrant: 'call' });
  });

  it('uses the repository root as the upload workspace, and none at all in the home folder', async () => {
    const root = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-upload-root-')));
    try {
      fs.mkdirSync(path.join(root, '.git'));
      fs.mkdirSync(path.join(root, 'pkg'));
      fs.writeFileSync(path.join(root, 'top.txt'), 't');
      // From a subfolder, a file elsewhere in the repository is inside the workspace: no outside reason.
      const inside = ui(true);
      await checkUploadPaths(['../top.txt'], { ...inside, cwd: path.join(root, 'pkg') }, fakePermission('workspace'));
      expect(inside.select.mock.calls[0]![0]).toMatch(/^Upload files to the page\?/);
      // A session whose cwd is the home folder has no workspace: even a file under it is outside.
      const home = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-upload-home-')));
      vi.stubEnv('HOME', home);
      vi.stubEnv('USERPROFILE', home);
      fs.writeFileSync(path.join(home, 'notes.txt'), 'n');
      const homeCtx = ui(true);
      await checkUploadPaths(['notes.txt'], { ...homeCtx, cwd: home }, fakePermission('workspace'));
      expect(homeCtx.select.mock.calls[0]![0]).toMatch(/^Upload files from outside the workspace\?/);
      fs.rmSync(home, { recursive: true, force: true });
    } finally {
      vi.unstubAllEnvs();
      fs.rmSync(root, { recursive: true, force: true });
    }
  });

  it('follows symlinks before deciding a file is inside the workspace, and uploads the real file', async () => {
    const root = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-upload-')));
    try {
      const workspace = path.join(root, 'work');
      fs.mkdirSync(workspace);
      fs.writeFileSync(path.join(root, 'secret'), 's');
      fs.writeFileSync(path.join(workspace, 'a.txt'), 'a');
      fs.symlinkSync(path.join(root, 'secret'), path.join(workspace, 'link'));
      fs.symlinkSync(root, path.join(workspace, 'up'));
      const ctx = { ...ui(false), cwd: workspace };
      const permission = fakePermission('workspace');
      await expect(checkUploadPaths(['link'], ctx, permission)).rejects.toThrow(`Refused to upload files outside the workspace: ${path.join(root, 'secret')}.`);
      await expect(checkUploadPaths(['up/secret'], ctx, permission)).rejects.toThrow(/outside the workspace/);
      expect(await checkUploadPaths(['link'], { ...ui(true), cwd: workspace }, permission)).toEqual([path.join(root, 'secret')]);
      // A workspace reached through a symlink still holds its own files.
      fs.symlinkSync(workspace, path.join(root, 'alias'));
      expect(await checkUploadPaths(['a.txt', 'new.txt'], { ...ui(true), cwd: path.join(root, 'alias') }, permission)).toEqual([path.join(workspace, 'a.txt'), path.join(workspace, 'new.txt')]);
    } finally {
      fs.rmSync(root, { recursive: true, force: true });
    }
  });
});
