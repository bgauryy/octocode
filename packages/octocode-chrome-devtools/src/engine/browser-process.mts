// The local Chrome process: finding the executable, its pid and command line, and stopping it.
import { execSync, execFileSync } from 'child_process';
import { platform } from 'os';
import { existsSync, rmSync } from 'fs';

const MAC_CHROMES = [
  '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
  '/Applications/Chromium.app/Contents/MacOS/Chromium',
  '/Applications/Google Chrome Canary.app/Contents/MacOS/Google Chrome Canary',
  '/Applications/Brave Browser.app/Contents/MacOS/Brave Browser',
];
const LINUX_CHROMES = [
  'google-chrome',
  'google-chrome-stable',
  'chromium-browser',
  'chromium',
];

function linuxChrome() {
  for (const bin of LINUX_CHROMES) {
    try {
      execSync(`which ${bin}`, { stdio: 'ignore' });
      return bin;
    } catch {
      // not on PATH; try the next name
    }
  }
  return null;
}

function windowsChrome() {
  const candidates = [
    `${process.env.LOCALAPPDATA}\\Google\\Chrome\\Application\\chrome.exe`,
    `${process.env.PROGRAMFILES}\\Google\\Chrome\\Application\\chrome.exe`,
    `${process.env['PROGRAMFILES(X86)']}\\Google\\Chrome\\Application\\chrome.exe`,
    `${process.env.LOCALAPPDATA}\\Chromium\\Application\\chrome.exe`,
  ];
  return candidates.find(p => p && existsSync(p)) ?? null;
}

/** The installed Chrome executable for this OS, or null. */
export function findChrome() {
  if (platform() === 'darwin')
    return MAC_CHROMES.find(p => existsSync(p)) ?? null;
  if (platform() === 'linux') {
    const bin = linuxChrome();
    if (bin) return bin;
  }
  if (platform() === 'win32') return windowsChrome();
  return null;
}

// The OS pid of the browser process serving this CDP endpoint.
export async function browserPid(
  wsUrl: unknown,
  timeoutMs = 3000
): Promise<number | null> {
  if (typeof wsUrl !== 'string' || !wsUrl) return null;
  return new Promise<number | null>(resolvePid => {
    let socket: WebSocket | undefined;
    const done = (pid: number | null) => {
      clearTimeout(timer);
      try {
        socket?.close();
      } catch {
        // already closed
      }
      resolvePid(pid);
    };
    const timer = setTimeout(() => done(null), timeoutMs);
    try {
      socket = new WebSocket(wsUrl);
    } catch {
      done(null);
      return;
    }
    // A const keeps the narrowed socket for the listeners below.
    const ws = socket;
    ws.addEventListener('open', () =>
      ws.send(JSON.stringify({ id: 1, method: 'SystemInfo.getProcessInfo' }))
    );
    ws.addEventListener('error', () => done(null));
    ws.addEventListener('close', () => done(null));
    ws.addEventListener('message', event => {
      try {
        const message = JSON.parse(String(event.data));
        if (message.id !== 1) return;
        const browser = message.result?.processInfo?.find(
          (info: { type?: unknown }) => info.type === 'browser'
        );
        done(Number.isSafeInteger(browser?.id) ? browser.id : null);
      } catch {
        done(null);
      }
    });
  });
}

function readCommand(pidText: string) {
  if (platform() === 'darwin' || platform() === 'linux')
    return execFileSync('ps', ['-p', pidText, '-o', 'command='], {
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'ignore'],
    }).trim();
  if (platform() === 'win32')
    return execFileSync(
      'powershell.exe',
      [
        '-NoProfile',
        '-Command',
        `(Get-CimInstance Win32_Process -Filter "ProcessId=${pidText}").CommandLine`,
      ],
      { encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'] }
    ).trim();
  return '';
}

/** The command line of a running process; empty when unknown. */
export function getProcessCommand(pid: unknown) {
  const pidText = String(pid);
  if (!/^\d+$/.test(pidText)) return '';
  try {
    return readCommand(pidText);
  } catch {
    return '';
  }
}

/** Waits until pid is gone; false when it is still alive after timeoutMs. */
export async function waitForExit(pid: number, timeoutMs = 3000) {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    try {
      process.kill(pid, 0);
      await new Promise(r => setTimeout(r, 100));
    } catch {
      return true;
    }
  }
  return false;
}

/** Removes a profile folder, retrying while Chrome releases its files. */
export async function removeDirWithRetry(dir: string) {
  for (let attempt = 0; attempt < 5; attempt++) {
    try {
      rmSync(dir, { recursive: true, force: true });
      if (!existsSync(dir)) return true;
    } catch (e) {
      if (attempt === 4) throw e;
    }
    await new Promise(r => setTimeout(r, 200));
  }
  return !existsSync(dir);
}
