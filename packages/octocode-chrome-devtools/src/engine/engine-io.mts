// Process-level IO shared by the standalone engines: the workspace `.octocode` root and Chrome's DevTools HTTP endpoint.
import { existsSync, mkdirSync } from 'fs';
import { join, resolve } from 'path';

/** The workspace folder's name, and this package's run tree below it. */
export const OCTOCODE_DIR = '.octocode';
export const PACKAGE_TMP = ['octocode-chrome-devtools', 'tmp'] as const;

/** The workspace folder every engine keeps its reports and run folders under. */
export const octocodeWorkspace = () => resolve(process.cwd(), OCTOCODE_DIR);

/** This package's run tree (run folders, session state, staged helpers) under a workspace folder. */
export const packageTmpDir = (octocodeDir = octocodeWorkspace()) =>
  join(octocodeDir, ...PACKAGE_TMP);

/**
 * The workspace folder, created when missing. A sandboxed engine finds it already made and cannot write
 * its root (only `octocode-chrome-devtools/tmp` below it), so an existing folder is never touched.
 */
export function octocodeOutputBase() {
  const workspace = octocodeWorkspace();
  if (!existsSync(workspace))
    mkdirSync(workspace, { recursive: true, mode: 0o700 });
  return workspace;
}

/**
 * One request to Chrome's DevTools HTTP endpoint. The numeric loopback address is what Chrome binds
 * first, and it avoids resolving `localhost` to an IPv6 address Chrome may not listen on.
 */
export async function cdpRequest(
  port: number | string,
  path: string,
  { method = 'GET', timeoutMs = 5000 } = {}
) {
  const res = await fetch(`http://127.0.0.1:${port}${path}`, {
    method,
    signal: AbortSignal.timeout(timeoutMs),
  });
  if (!res.ok) throw new Error(`CDP HTTP ${res.status} for ${path}`);
  return res;
}

/** A DevTools HTTP endpoint that answers with JSON (`/json/version`, `/json`, `/json/new`). */
export async function cdpHttp(
  port: number | string,
  path: string,
  options?: { method?: string; timeoutMs?: number }
) {
  return (await cdpRequest(port, path, options)).json();
}
