// Shared helpers for the octocode-brainstorming web-search scripts
// (../tavily-search.mjs, ../serper-search.mjs, ../exa-search.mjs).
// Engine-specific arg parsing, request bodies, and response normalization
// stay in each script; only env-loading/API-key/error-reporting boilerplate
// that was previously duplicated three times lives here.

import { resolve } from 'node:path';

export function die(msg, code = 1) {
  process.stderr.write(`ERROR: ${msg}\n`);
  process.exitCode = code;
}

// Unified env loading via octocode-config.mjs (injected by skills/scripts/sync.mjs
// into ../octocode-config.mjs, i.e. scripts/octocode-config.mjs — one level up from
// this lib/ file — for every skill's scripts/ dir).
//
// Priority (highest → lowest):
//   1. process.env already set (shell / MCP client / pi-extension session_start)
//   2. <workspace>/.octocode/.env   (project-level, WORKSPACE_ROOT or cwd)
//   3. <octocode-home>/.env         (global; getOctocodeHome() — macOS ~/.octocode,
//                                    Linux ${XDG_CONFIG_HOME:-~/.config}/.octocode,
//                                    Windows %APPDATA%\.octocode, override OCTOCODE_HOME)
// Project env wins over global; already-set process.env vars always win over both.
export async function loadEnv() {
  const { propagateOctocodeEnv, getOctocodeHome } = await import(new URL('../octocode-config.mjs', import.meta.url).href);
  const home = getOctocodeHome();
  propagateOctocodeEnv({
    home,
    cwd: process.env.WORKSPACE_ROOT || process.cwd(),
    trusted: true,
  });
  return resolve(home, '.env');
}

export function splitList(v) {
  return String(v || '').split(',').map(s => s.trim()).filter(Boolean);
}

// Defends against a key pasted with an accidental "Bearer " or "Authorization: "
// prefix — all three engines take a bare key, just via different header shapes.
export function normalizeApiKey(raw) {
  let key = String(raw || '').trim();
  key = key.replace(/^Authorization\s*:\s*/i, '').trim();
  key = key.replace(/^Bearer\s+/i, '').trim();
  return key;
}
