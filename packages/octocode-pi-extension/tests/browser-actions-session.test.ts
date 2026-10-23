import fs from 'node:fs';
import path from 'node:path';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { answering, stub } from './doubles.js';
import { tmp } from './helpers.js';
import { sessionOutputDir } from '../src/shared/home.js';

const { answerDialog, download, uploadFiles } = await import('../src/browser/actions.js');
type Session = Parameters<typeof download>[0];
type Sent = { method: string; params: Record<string, unknown> };

/** A page that answers `send` from `reply`, records it, and lets the test emit events. */
function fakeSession(reply: (method: string, params: Record<string, unknown>) => unknown = () => ({})) {
  const listeners = new Set<(method: string, params: Record<string, unknown>) => void>();
  const sent: Sent[] = [];
  const page = {
    on: (listener: (method: string, params: Record<string, unknown>) => void) => (listeners.add(listener), () => listeners.delete(listener)),
    send: answering((method: string, params: Record<string, unknown> = {}) => (sent.push({ method, params }), reply(method, params) ?? {})),
  };
  const session = stub<Session>({ page: stub<Session['page']>(page), downloads: new Map(), console: [] });
  return { session, page: session.page, sent, listeners };
}
const element = 'document.body';
const downloadsDir = () => path.join(sessionOutputDir('output'), 'downloads');

afterEach(() => vi.useRealTimers());

const button = (method: string) => (method === 'Runtime.evaluate' ? { result: { objectId: 'obj' } } : method === 'Runtime.callFunctionOn' ? { result: { value: false } } : {});

describe('browser actions without a browser', () => {
  it('refuses to answer when no dialog is open, and passes the prompt reply when one is', async () => {
    const { session, sent } = fakeSession();
    await expect(answerDialog(session, true, undefined)).rejects.toThrow('No dialog is open.');
    session.dialog = { type: 'prompt', message: 'Name?' };
    expect(await answerDialog(session, false, 'Ada')).toBe('Dismissed the prompt dialog.');
    expect(sent).toEqual([{ method: 'Page.handleJavaScriptDialog', params: { accept: false, promptText: 'Ada' } }]);
    expect(session.dialog).toBeUndefined();
  });

  it('refuses an upload with no files, a missing file, or a gone element', async () => {
    const dir = tmp('octocode-browser-actions-');
    fs.writeFileSync(path.join(dir, 'a.txt'), 'a');
    const { page } = fakeSession(() => ({ result: {} }));
    await expect(uploadFiles(page, 1, element, [])).rejects.toThrow('upload needs files');
    await expect(uploadFiles(page, 1, element, [path.join(dir, 'missing.txt')])).rejects.toThrow(`No such file: ${path.join(dir, 'missing.txt')}`);
    await expect(uploadFiles(page, 1, element, [path.join(dir, 'a.txt')])).rejects.toThrow('No element [1] — take a new snapshot.');
  });

  /** A non-input element: clicking it may or may not open a file chooser. */
  it('says when clicking an upload button opened no file chooser, and turns interception back off', async () => {
    const dir = tmp('octocode-browser-actions-');
    fs.writeFileSync(path.join(dir, 'a.txt'), 'a');
    const { page, sent } = fakeSession(button);
    vi.useFakeTimers();
    const upload = expect(uploadFiles(page, 2, element, [path.join(dir, 'a.txt')])).rejects.toThrow('Clicking [2] opened no file chooser; use the file input instead.');
    await vi.advanceTimersByTimeAsync(5_000);
    await upload;
    expect(sent.filter((item) => item.method === 'Page.setInterceptFileChooserDialog').map((item) => item.params['enabled'])).toEqual([true, false]);
  });

  it('stops waiting for the file chooser when aborted, before or during the wait', async () => {
    const dir = tmp('octocode-browser-actions-');
    fs.writeFileSync(path.join(dir, 'a.txt'), 'a');
    const before = new AbortController();
    before.abort(new Error('already stopped'));
    const first = fakeSession(button);
    await expect(uploadFiles(first.page, 2, element, [path.join(dir, 'a.txt')], before.signal)).rejects.toThrow('already stopped');
    const during = new AbortController();
    const second = fakeSession((method) => {
      if (method === 'Runtime.callFunctionOn' && second.sent.filter((item) => item.method === method).length === 2) setTimeout(() => during.abort(), 5);
      return button(method);
    });
    await expect(uploadFiles(second.page, 2, element, [path.join(dir, 'a.txt')], during.signal)).rejects.toThrow(/aborted/i);
    expect(second.listeners.size).toBe(0);
    expect(second.sent.at(-1)).toEqual({ method: 'Page.setInterceptFileChooserDialog', params: { enabled: false } });
  });

  it('turns downloads on once, into the session\'s private folder, and says when there are none yet', async () => {
    const dir = downloadsDir();
    const { session, sent } = fakeSession();
    expect(await download(session, undefined)).toBe(`No downloads yet (saving to ${dir}). Pass ref to click a download link.`);
    expect(fs.statSync(dir).mode & 0o777).toBe(0o700);
    expect(session.downloadDir).toBe(dir);
    await download(session, undefined);
    expect(sent.map((item) => item.method)).toEqual(['Browser.setDownloadBehavior']);
  });

  it('stops waiting for a running download when aborted, and lists what finished with its path', async () => {
    const dir = downloadsDir();
    const { session } = fakeSession();
    session.downloads.set('g', { name: 'big.zip', state: 'inProgress', bytes: 1 });
    const controller = new AbortController();
    setTimeout(() => controller.abort(new Error('enough')), 20);
    await expect(download(session, undefined, controller.signal)).rejects.toThrow('enough');
    session.downloads.set('g', { name: 'big.zip', state: 'canceled', bytes: 1 });
    fs.writeFileSync(path.join(dir, 'ok.csv'), 'a,b\n');
    session.downloads.set('h', { name: 'ok.csv', state: 'completed', bytes: 12 });
    // Chrome's reported path wins over the suggested name (taken names get a suffix).
    fs.writeFileSync(path.join(dir, 'ok (1).csv'), 'a,b\n');
    session.downloads.set('i', { name: 'ok.csv', state: 'completed', bytes: 4, path: path.join(dir, 'ok (1).csv') });
    expect(await download(session, undefined)).toBe(
      `Downloads in ${dir}:\n- big.zip · 1 bytes · canceled\n- ok.csv · 12 bytes · saved · ${path.join(dir, 'ok.csv')}\n- ok.csv · 4 bytes · saved · ${path.join(dir, 'ok (1).csv')}`,
    );
  });
});
