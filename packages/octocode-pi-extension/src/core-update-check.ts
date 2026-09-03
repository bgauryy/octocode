/**
 * Checks whether a newer @octocodeai/pi-extension is published on npm.
 *
 * Mirrors Pi's own DefaultPackageManager#checkForAvailableUpdates (the mechanism
 * behind Pi's "package update available" banner): shells out to
 * `npm view <pkg> version --json` — this respects the user's .npmrc/private
 * registry/proxy config rather than hardcoding a registry URL — with a 10s
 * timeout, gated on PI_OFFLINE, and never throws.
 *
 * Pi's own package-manager only checks packages registered in Pi's own
 * settings.json `packages` list (things installed via `pi install`).
 * @octocodeai/pi-extension is a hard npm dependency loaded via
 * extensionFactories, not a Pi package, so Pi's mechanism never sees it —
 * this fills that gap. Callers are expected to invoke this fire-and-forget
 * (never awaited before the session becomes usable) and only when
 * ctx.hasUI is true, matching where Pi wires its own equivalent checks
 * (interactive-mode.js#run, never print/rpc mode).
 */
import { spawn } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';

export const CORE_PACKAGE_NAME = '@octocodeai/pi-extension';
const NPM_VIEW_TIMEOUT_MS = 10_000;

export interface CoreUpdateInfo {
  currentVersion: string;
  latestVersion: string;
}

export interface CoreUpdateCheckDeps {
  runNpmView?: (packageName: string) => Promise<string>;
}

function isTruthyEnvFlag(value: string | undefined): boolean {
  if (!value) return false;
  return value === '1' || value.toLowerCase() === 'true' || value.toLowerCase() === 'yes';
}

/** Same PI_OFFLINE convention Pi's own version/package checks honor. */
export function isUpdateCheckDisabled(env: NodeJS.ProcessEnv = process.env): boolean {
  return isTruthyEnvFlag(env.PI_OFFLINE);
}

/** Reads this extension's own version from its package.json (one level above baseDir/dist). */
const ownVersionCache = new Map<string, string | undefined>();

/** Memoized: the banner entry renderer calls this per frame; the file never changes mid-process. */
export function readOwnVersion(baseDir: string): string | undefined {
  if (ownVersionCache.has(baseDir)) return ownVersionCache.get(baseDir);
  const v = readOwnVersionUncached(baseDir);
  ownVersionCache.set(baseDir, v);
  return v;
}

function readOwnVersionUncached(baseDir: string): string | undefined {
  try {
    const pkgPath = path.join(path.dirname(baseDir), 'package.json');
    const pkg = JSON.parse(fs.readFileSync(pkgPath, 'utf8')) as { version?: string };
    return typeof pkg.version === 'string' && pkg.version.trim() ? pkg.version.trim() : undefined;
  } catch {
    return undefined;
  }
}

function defaultRunNpmView(packageName: string): Promise<string> {
  return new Promise((resolve, reject) => {
    const child = spawn('npm', ['view', packageName, 'version', '--json'], {
      stdio: ['ignore', 'pipe', 'ignore'],
    });
    let stdout = '';
    let settled = false;
    const finish = (fn: () => void) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      fn();
    };
    const timer = setTimeout(() => {
      finish(() => {
        child.kill();
        reject(new Error('npm view timed out'));
      });
    }, NPM_VIEW_TIMEOUT_MS);
    child.stdout?.on('data', (chunk: Buffer) => {
      stdout += chunk.toString();
    });
    child.on('error', (err) => finish(() => reject(err)));
    child.on('close', (code) => {
      finish(() => {
        if (code !== 0) {
          reject(new Error(`npm view exited with code ${code}`));
          return;
        }
        resolve(stdout.trim());
      });
    });
  });
}

/** Parses `npm view <pkg> version --json` output: a bare JSON-quoted version string. */
export function parseNpmViewVersion(raw: string): string | undefined {
  if (!raw.trim()) return undefined;
  try {
    const parsed: unknown = JSON.parse(raw);
    if (typeof parsed === 'string' && parsed.trim()) return parsed.trim();
  } catch {
    /* malformed output — treat as no answer, never throw */
  }
  return undefined;
}

/**
 * Checks for a newer @octocodeai/pi-extension on npm.
 *
 * Returns undefined — never throws — when offline, on any network/parse
 * failure, when the current version is unknown, or when already up to date.
 * Matches Pi's own package-update comparison (`!==`, not a semver `>` check):
 * any difference from the registry's reported version is treated as "an
 * update is available," including when the installed copy is a local/dev
 * build ahead of what's published.
 */
export async function checkForCoreUpdate(
  currentVersion: string | undefined,
  env: NodeJS.ProcessEnv = process.env,
  deps: CoreUpdateCheckDeps = {},
): Promise<CoreUpdateInfo | undefined> {
  if (!currentVersion || isUpdateCheckDisabled(env)) return undefined;
  try {
    const raw = await (deps.runNpmView ?? defaultRunNpmView)(CORE_PACKAGE_NAME);
    const latestVersion = parseNpmViewVersion(raw);
    if (!latestVersion || latestVersion === currentVersion) return undefined;
    return { currentVersion, latestVersion };
  } catch {
    return undefined;
  }
}
