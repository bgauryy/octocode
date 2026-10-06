import fs from 'node:fs';
import path from 'node:path';
import { abortError, type BrowserSession } from './cdp.js';
import { applyDownloads } from './emulation.js';

type Page = BrowserSession['page'];

/** What a snapshot shows while a JavaScript dialog blocks the page (evaluating anything would hang until it is answered). */
export function dialogNotice(dialog: NonNullable<BrowserSession['dialog']>): string {
  return `A ${dialog.type} dialog is open: ${JSON.stringify(dialog.message)}\nThe page is blocked until you answer it: dialog (accept true or false; text is the prompt reply).`;
}

/** Accepts or dismisses the open dialog; `text` answers a prompt(). */
export async function answerDialog(session: BrowserSession, accept: boolean, text: string | undefined, signal?: AbortSignal): Promise<string> {
  const open = session.dialog;
  if (!open) throw new Error('No dialog is open.');
  await session.page.send('Page.handleJavaScriptDialog', { accept, ...(text !== undefined ? { promptText: text } : {}) }, signal);
  delete session.dialog;
  return `${accept ? 'Accepted' : 'Dismissed'} the ${open.type} dialog.`;
}

/** Resolves with the params of the first `method` event, or `undefined` after `timeoutMs`. */
function nextEvent(page: Page, method: string, timeoutMs: number, signal?: AbortSignal): Promise<Record<string, unknown> | undefined> {
  return new Promise((resolve, reject) => {
    if (signal?.aborted) return reject(abortError(signal));
    const stop = () => {
      clearTimeout(timer);
      off();
      signal?.removeEventListener('abort', onAbort);
    };
    const finish = (value: Record<string, unknown> | undefined) => {
      stop();
      resolve(value);
    };
    const onAbort = () => {
      stop();
      reject(abortError(signal!));
    };
    const timer = setTimeout(() => finish(undefined), timeoutMs);
    const off = page.on((name, params) => name === method && finish(params));
    signal?.addEventListener('abort', onAbort, { once: true });
  });
}

/** Sets `files` on the file input `[ref]`, or on the file chooser that clicking `[ref]` opens (an upload button). */
export async function uploadFiles(page: Page, ref: number, element: string, files: string[] | undefined, cwd: string, signal?: AbortSignal): Promise<string> {
  if (!files || files.length === 0) throw new Error('upload needs files (paths to attach)');
  const paths = files.map((file) => path.resolve(cwd, file));
  const missing = paths.find((file) => !fs.statSync(file, { throwIfNoEntry: false })?.isFile());
  if (missing) throw new Error(`No such file: ${missing}`);
  const found = await page.send<{ result?: { objectId?: string } }>('Runtime.evaluate', { expression: element }, signal);
  const objectId = found.result?.objectId;
  if (!objectId) throw new Error(`No element [${ref}] — take a new snapshot.`);
  const call = (body: string) => page.send<{ result?: { value?: unknown } }>('Runtime.callFunctionOn', { objectId, functionDeclaration: `function () { ${body} }`, returnByValue: true, userGesture: true }, signal);
  const isFileInput = (await call("return this.tagName === 'INPUT' && this.type === 'file';")).result?.value === true;
  if (isFileInput) {
    await page.send('DOM.setFileInputFiles', { files: paths, objectId }, signal);
  } else {
    await page.send('Page.setInterceptFileChooserDialog', { enabled: true }, signal);
    try {
      const chooser = nextEvent(page, 'Page.fileChooserOpened', 5_000, signal);
      chooser.catch(() => undefined);
      await call("this.scrollIntoView({ block: 'center' }); this.click();");
      const opened = await chooser;
      if (!opened) throw new Error(`Clicking [${ref}] opened no file chooser; use the file input instead.`);
      await page.send('DOM.setFileInputFiles', { files: paths, backendNodeId: opened['backendNodeId'] }, signal);
    } finally {
      await page.send('Page.setInterceptFileChooserDialog', { enabled: false }).catch(() => undefined);
    }
  }
  return `Attached to [${ref}]: ${paths.map((file) => path.basename(file)).join(', ')}`;
}

/** Polls `done` every 100 ms until it holds or `timeoutMs` passes; true when it held. */
async function until(done: () => boolean, timeoutMs: number, signal?: AbortSignal): Promise<boolean> {
  const deadline = Date.now() + timeoutMs;
  while (!done()) {
    if (signal?.aborted) throw abortError(signal);
    if (Date.now() >= deadline) return false;
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  return true;
}

/**
 * Turns downloads on (saved to `<cwd>/downloads`), optionally runs `click` to start one, waits for running downloads
 * to finish, and lists them.
 */
export async function download(session: BrowserSession, cwd: string, click: (() => Promise<void>) | undefined, signal?: AbortSignal, waitMs = 60_000): Promise<string> {
  if (!session.downloadDir) {
    const dir = path.join(cwd, 'downloads');
    fs.mkdirSync(dir, { recursive: true });
    await applyDownloads(session.page, dir, signal);
    // Set from now on: switchTo applies it to every tab the session moves to.
    session.downloadDir = dir;
  }
  const before = session.downloads.size;
  if (click) {
    await click();
    await until(() => session.downloads.size > before, 5_000, signal);
  }
  const running = () => [...session.downloads.values()].some((item) => item.state === 'inProgress');
  await until(() => !running(), waitMs, signal);
  const items = [...session.downloads.values()];
  if (items.length === 0) return `No downloads yet (saving to ${session.downloadDir}). Pass ref to click a download link.`;
  const state = { completed: 'saved', canceled: 'canceled', inProgress: 'still downloading' } as const;
  return [`Downloads in ${session.downloadDir}:`, ...items.map((item) => `- ${item.name} · ${item.bytes} bytes · ${state[item.state] ?? item.state}`)].join('\n');
}
