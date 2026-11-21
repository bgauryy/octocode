// A throwaway headless Chrome for tests that read its live protocol: free port, isolated profile,
// PID recorded and killed by PID. `chromeBinary` is undefined when no Chrome is installed (skip).
import { spawn } from 'node:child_process';
import { existsSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { delimiter, join } from 'node:path';
import { freePort } from './free-port.mjs';

const candidates = [
  '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
  ...process.env.PATH.split(delimiter).flatMap(dir =>
    ['google-chrome', 'chromium', 'chrome'].map(name => join(dir, name))
  ),
];
export const chromeBinary = candidates.find(existsSync);

/** Launch Chrome; resolves with its port, pid, version, live protocol and an stop() (awaited). */
export async function startThrowawayChrome() {
  const profile = mkdtempSync(join(tmpdir(), 'octo-throwaway-chrome-'));
  const port = await freePort();
  const child = spawn(
    chromeBinary,
    [
      '--headless=new',
      `--remote-debugging-port=${port}`,
      `--user-data-dir=${profile}`,
      '--no-first-run',
      '--no-sandbox',
      'about:blank',
    ],
    { stdio: 'ignore' }
  );
  const pid = child.pid;
  const exited = new Promise(resolve => child.once('exit', resolve));
  const stop = async () => {
    if (child.exitCode === null) {
      process.kill(pid);
      await exited;
    }
    rmSync(profile, {
      recursive: true,
      force: true,
      maxRetries: 10,
      retryDelay: 100,
    });
  };
  try {
    const base = `http://127.0.0.1:${port}`;
    for (let attempt = 0; attempt < 100; attempt++) {
      try {
        const version = await (await fetch(`${base}/json/version`)).json();
        const protocol = await (await fetch(`${base}/json/protocol`)).json();
        return { port, pid, version: version.Browser, protocol, stop };
      } catch {
        await new Promise(resolve => setTimeout(resolve, 200));
      }
    }
    throw new Error(`Chrome pid ${pid} did not answer on port ${port}`);
  } catch (error) {
    await stop();
    throw error;
  }
}
