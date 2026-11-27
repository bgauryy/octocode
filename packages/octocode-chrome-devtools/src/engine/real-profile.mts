/**
 * Real-profile launch rules. A real profile belongs to the user's own browser, so a
 * launch must target that browser's data directory, never create profiles in it, and
 * fail clearly when the browser holds it or refuses remote debugging there.
 */
import { execFileSync } from 'child_process';
import { existsSync, readdirSync, readlinkSync } from 'fs';
import { basename, dirname, join } from 'path';

type BrowserFamily =
  'chrome' | 'chrome-canary' | 'chrome-for-testing' | 'chromium' | 'brave';

const FAMILY_PATTERNS: [BrowserFamily, RegExp][] = [
  ['chrome-for-testing', /chrome for testing|chrome-for-testing/i],
  ['chrome-canary', /chrome canary|chrome-canary|chrome sxs/i],
  ['brave', /brave/i],
  ['chromium', /chromium/i],
  ['chrome', /google.chrome|google-chrome|chrome\.exe$|[\\/]chrome$/i],
];

/** Branded Google Chrome ignores remote debugging on its default data dir from 136. */
const REMOTE_DEBUGGING_DEFAULT_DIR_BLOCKED_FROM = 136;
const BRANDED: BrowserFamily[] = ['chrome', 'chrome-canary'];

export function browserFamily(chromePath: string): BrowserFamily | null {
  return FAMILY_PATTERNS.find(([, re]) => re.test(chromePath))?.[0] ?? null;
}

export function realProfileDataDir(
  family: BrowserFamily,
  os: NodeJS.Platform,
  env: NodeJS.ProcessEnv
): string | null {
  const home = env.HOME ?? env.USERPROFILE;
  if (os === 'darwin') {
    if (!home) return null;
    const base = join(home, 'Library', 'Application Support');
    return {
      chrome: join(base, 'Google', 'Chrome'),
      'chrome-canary': join(base, 'Google', 'Chrome Canary'),
      'chrome-for-testing': join(base, 'Google', 'Chrome for Testing'),
      chromium: join(base, 'Chromium'),
      brave: join(base, 'BraveSoftware', 'Brave-Browser'),
    }[family];
  }
  if (os === 'win32') {
    if (!env.LOCALAPPDATA) return null;
    const base = env.LOCALAPPDATA;
    return {
      chrome: join(base, 'Google', 'Chrome', 'User Data'),
      'chrome-canary': join(base, 'Google', 'Chrome SxS', 'User Data'),
      'chrome-for-testing': join(
        base,
        'Google',
        'Chrome for Testing',
        'User Data'
      ),
      chromium: join(base, 'Chromium', 'User Data'),
      brave: join(base, 'BraveSoftware', 'Brave-Browser', 'User Data'),
    }[family];
  }
  if (!home) return null;
  const base = env.XDG_CONFIG_HOME || join(home, '.config');
  return {
    chrome: join(base, 'google-chrome'),
    'chrome-canary': join(base, 'google-chrome-unstable'),
    'chrome-for-testing': join(base, 'google-chrome-for-testing'),
    chromium: join(base, 'chromium'),
    brave: join(base, 'BraveSoftware', 'Brave-Browser'),
  }[family];
}

/** Major version from `--version` output, or the version folder next to chrome.exe. */
export function browserMajorVersion(
  chromePath: string,
  os: NodeJS.Platform
): number | null {
  try {
    if (os === 'win32') {
      const versions = readdirSync(dirname(chromePath))
        .map(name => /^(\d+)\.\d+\.\d+\.\d+$/.exec(name)?.[1])
        .filter(Boolean)
        .map(Number);
      return versions.length ? Math.max(...versions) : null;
    }
    const out = execFileSync(chromePath, ['--version'], {
      encoding: 'utf8',
      timeout: 10000,
      stdio: ['ignore', 'pipe', 'ignore'],
    });
    const match = /(\d+)\.\d+\.\d+(?:\.\d+)?/.exec(out);
    return match ? Number(match[1]) : null;
  } catch {
    return null;
  }
}

export function remoteDebuggingBlocked(
  family: BrowserFamily,
  major: number | null
): boolean {
  return (
    BRANDED.includes(family) &&
    major !== null &&
    major >= REMOTE_DEBUGGING_DEFAULT_DIR_BLOCKED_FROM
  );
}

const alive = (pid: number) => {
  try {
    process.kill(pid, 0);
    return true;
  } catch (error) {
    return (error as NodeJS.ErrnoException).code === 'EPERM';
  }
};

/**
 * Whether a running browser holds the data dir. POSIX Chrome keeps a SingletonLock
 * symlink to "<host>-<pid>"; Windows keeps a "lockfile" open while it runs.
 */
export function dataDirInUse(dataDir: string, os: NodeJS.Platform): boolean {
  if (os === 'win32') return existsSync(join(dataDir, 'lockfile'));
  let target: string;
  try {
    target = readlinkSync(join(dataDir, 'SingletonLock'));
  } catch {
    return false;
  }
  const pid = Number(/-(\d+)$/.exec(basename(target))?.[1]);
  return Number.isInteger(pid) && pid > 0 ? alive(pid) : true;
}

/** Profile directories Chrome created in the data dir (Default, Profile 1, ...). */
function listProfiles(dataDir: string): string[] {
  try {
    return readdirSync(dataDir, { withFileTypes: true })
      .filter(
        entry =>
          entry.isDirectory() &&
          (entry.name === 'Default' || /^Profile \d+$/.test(entry.name)) &&
          existsSync(join(dataDir, entry.name, 'Preferences'))
      )
      .map(entry => entry.name);
  } catch {
    return [];
  }
}

/**
 * Resolves the real-profile launch or the reason it must not happen. Never returns a
 * plan that would create a profile or share a data dir with a running browser.
 */
export function planRealProfile({
  chromePath,
  profile,
  os,
  env,
  major,
}: {
  chromePath: string;
  profile: string;
  os: NodeJS.Platform;
  env: NodeJS.ProcessEnv;
  major: () => number | null;
}): { userDataDir: string; family: BrowserFamily } | { error: string } {
  const family = browserFamily(chromePath);
  if (!family)
    return {
      error: `Cannot locate the real profile for ${chromePath}: only Google Chrome, Chrome Canary, Chrome for Testing, Chromium and Brave data directories are known.`,
    };
  const userDataDir = realProfileDataDir(family, os, env);
  if (!userDataDir)
    return {
      error: `Cannot resolve the ${family} data directory: HOME/LOCALAPPDATA is unset.`,
    };
  if (remoteDebuggingBlocked(family, major()))
    return {
      error: `Google Chrome ${REMOTE_DEBUGGING_DEFAULT_DIR_BLOCKED_FROM}+ ignores --remote-debugging-port on its default profile directory (https://developer.chrome.com/blog/remote-debugging-port), so the real profile cannot be driven. Sign in inside an isolated session (open without --profile), transfer cookies with cookies --from-port or --from-storage-state, or use Chromium / Chrome for Testing via --chromePath.`,
    };
  const profiles = listProfiles(userDataDir);
  if (!profiles.includes(profile))
    return {
      error: `Profile "${profile}" not found in ${userDataDir}${profiles.length ? `; available: ${profiles.join(', ')}` : ''}. Refusing to create a new profile in the real data directory.`,
    };
  if (dataDirInUse(userDataDir, os))
    return {
      error: `${family} is running and holds ${userDataDir}. Quit it fully before --profile, or keep it running and use cookies --from-port / --from-storage-state with an isolated session.`,
    };
  return { userDataDir, family };
}
