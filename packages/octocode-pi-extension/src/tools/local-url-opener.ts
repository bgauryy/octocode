import { spawn } from 'node:child_process';

export type LocalUrlOpenPreference = 'auto' | 'chrome' | 'system' | 'vscode' | 'none';
export type LocalUrlOpenedIn = 'chrome' | 'system' | 'vscode' | 'none';

export interface LocalUrlOpenResult {
  ok: boolean;
  requested: LocalUrlOpenPreference;
  openedIn: LocalUrlOpenedIn;
  message?: string;
}

interface OpenLocalUrlOptions {
  preference?: LocalUrlOpenPreference;
  env?: Record<string, string | undefined>;
  platform?: NodeJS.Platform;
  getChromeInstallations?: () => Promise<string[]>;
  openInVsCode?: (url: string) => Promise<boolean>;
  launch?: (command: string, args: string[]) => Promise<void>;
}

function isLoopbackUrl(target: string): boolean {
  try {
    const url = new URL(target);
    return (url.protocol === 'http:' || url.protocol === 'https:')
      && (url.hostname === '127.0.0.1' || url.hostname === 'localhost' || url.hostname === '[::1]');
  } catch {
    return false;
  }
}

function isVsCodeHost(env: Record<string, string | undefined>): boolean {
  return env['TERM_PROGRAM'] === 'vscode'
    || Boolean(env['VSCODE_IPC_HOOK'] || env['VSCODE_IPC_HOOK_CLI'] || env['VSCODE_GIT_IPC_HANDLE']);
}

async function defaultGetChromeInstallations(): Promise<string[]> {
  const moduleName = 'chrome-launcher';
  const chromeLauncher = await import(moduleName) as {
    Launcher?: { getInstallations?: () => string[] };
  };
  return chromeLauncher.Launcher?.getInstallations?.() ?? [];
}

async function defaultOpenInVsCode(target: string): Promise<boolean> {
  try {
    // The `vscode` module exists only in an extension host. Keeping the import
    // dynamic lets the same bundle run normally in a terminal process.
    const moduleName = 'vscode';
    const vscode = await import(moduleName) as {
      commands?: { executeCommand?: (command: string, ...args: unknown[]) => Promise<unknown> };
    };
    if (!vscode.commands?.executeCommand) return false;
    await vscode.commands.executeCommand('workbench.action.browser.open', target);
    return true;
  } catch {
    return false;
  }
}

function defaultLaunch(command: string, args: string[]): Promise<void> {
  return new Promise((resolve, reject) => {
    let child: ReturnType<typeof spawn>;
    try {
      child = spawn(command, args, { stdio: 'ignore', detached: true });
    } catch (error) {
      reject(error);
      return;
    }
    child.once('error', reject);
    child.once('spawn', () => {
      child.removeListener('error', reject);
      child.unref();
      resolve();
    });
  });
}

function systemCommand(platform: NodeJS.Platform, target: string): { command: string; args: string[] } {
  if (platform === 'darwin') return { command: 'open', args: [target] };
  if (platform === 'win32') return { command: 'cmd', args: ['/c', 'start', '', target] };
  return { command: 'xdg-open', args: [target] };
}

/** Open a loopback URL in the most native surface available for this host. */
export async function openLocalUrl(
  target: string,
  options: OpenLocalUrlOptions = {},
): Promise<LocalUrlOpenResult> {
  const requested = options.preference ?? 'auto';
  if (!isLoopbackUrl(target)) {
    return { ok: false, requested, openedIn: 'none', message: `Refusing to open a non-loopback URL: ${target}` };
  }
  if (requested === 'none') return { ok: true, requested, openedIn: 'none' };

  const env = options.env ?? process.env;
  const launch = options.launch ?? defaultLaunch;
  const getChromeInstallations = options.getChromeInstallations ?? defaultGetChromeInstallations;
  const openInVsCode = options.openInVsCode ?? defaultOpenInVsCode;

  if (requested === 'vscode' || (requested === 'auto' && isVsCodeHost(env))) {
    if (await openInVsCode(target)) return { ok: true, requested, openedIn: 'vscode' };
    if (requested === 'vscode') {
      return {
        ok: false,
        requested,
        openedIn: 'none',
        message: `VS Code's integrated-browser API is unavailable in this process. Open ${target} from VS Code instead.`,
      };
    }
  }

  if (requested === 'auto' || requested === 'chrome') {
    try {
      const [chrome] = await getChromeInstallations();
      if (chrome) {
        // Intentionally spawn the discovered binary directly. chrome-launcher's
        // launch() creates a temporary automation profile and debugging port.
        await launch(chrome, [target]);
        return { ok: true, requested, openedIn: 'chrome' };
      }
    } catch (error) {
      if (requested === 'chrome') {
        return { ok: false, requested, openedIn: 'none', message: `Could not open Chrome: ${(error as Error).message}. Open ${target} manually.` };
      }
    }
    if (requested === 'chrome') {
      return { ok: false, requested, openedIn: 'none', message: `Chrome was not found. Open ${target} manually.` };
    }
  }

  try {
    const { command, args } = systemCommand(options.platform ?? process.platform, target);
    await launch(command, args);
    return { ok: true, requested, openedIn: 'system' };
  } catch (error) {
    return {
      ok: false,
      requested,
      openedIn: 'none',
      message: `Could not open the local page: ${(error as Error).message}. Open ${target} manually.`,
    };
  }
}

/** Open an http(s) URL only after the caller has recorded explicit user consent. */
export async function openApprovedExternalUrl(
  target: string,
  approved: boolean,
): Promise<LocalUrlOpenResult> {
  const requested: LocalUrlOpenPreference = 'system';
  if (!approved) return { ok: false, requested, openedIn: 'none', message: 'External URL opening was not approved.' };
  let url: URL;
  try { url = new URL(target); } catch { return { ok: false, requested, openedIn: 'none', message: 'Invalid external URL.' }; }
  if (url.protocol !== 'https:' && url.protocol !== 'http:') {
    return { ok: false, requested, openedIn: 'none', message: `Refusing unsupported URL protocol: ${url.protocol}` };
  }
  try {
    const { command, args } = systemCommand(process.platform, url.href);
    await defaultLaunch(command, args);
    return { ok: true, requested, openedIn: 'system' };
  } catch (error) {
    return { ok: false, requested, openedIn: 'none', message: `Could not open authorization URL: ${(error as Error).message}` };
  }
}
