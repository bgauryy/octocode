// The hermetic world tool-health runs in: a temp HOME, the loopback mock, one fake
// credential per tool and surface, and a TLS proxy that answers registry hosts
// locally. The built CLI and MCP server return uniform response records.
import { spawnSync } from 'node:child_process';
import { AsyncLocalStorage } from 'node:async_hooks';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { ROOT } from '../paths.mjs';
import { spawnCli, startServer } from '../mcp-client.mjs';
import { contract } from '../contract.mjs';
import { statusClass } from './driver.mjs';
import { addBulkPullRequest } from './mock-model.mjs';
import { startMock } from './mock-server.mjs';
import { startRegistryProxy } from './mock-registry-proxy.mjs';

const DEAD_PROXY = 'http://127.0.0.1:9';
const ISOLATED_ENV = /^(GITHUB_|GH_|OCTOCODE_|NPM_CONFIG_|npm_config_|GIT_)/;
const ISOLATED_NAMES = new Set(['ALLOWED_PATHS', 'WORKSPACE_ROOT', 'TOOLS_TO_RUN', 'DISABLE_TOOLS']);

/** One fake credential per tool and surface: the runtime paces GitHub searches per credential. */
export const tokenFor = (tool, surface) => ['th', 'fake', tool, surface, String(process.pid)].join('-');

/** Startup may ping model readiness even when admission rejects the tool input. */
export const withoutReadinessPings = requests => requests.filter(entry => entry.request !== 'GET /v1/models');

function lspAvailable(fixture, env) {
  const names = ['tsgo', 'tsserver', 'typescript-language-server'];
  let typescript = false;
  for (let dir = fixture; ; dir = path.dirname(dir)) {
    if (names.some(n => fs.existsSync(path.join(dir, 'node_modules/.bin', n)))) { typescript = true; break; }
    if (path.dirname(dir) === dir) break;
  }
  if (!typescript && !names.some(n => spawnSync('which', [n], { env, encoding: 'utf8', timeout: 3_000 }).status === 0)) return 'no TypeScript language server (tsgo, tsserver or typescript-language-server) in node_modules/.bin or PATH';
  for (const command of ['rust-analyzer', 'cargo', 'rustc']) {
    if (spawnSync(command, ['--version'], { cwd: fixture, env, encoding: 'utf8', timeout: 3_000 }).status !== 0) return `${command} is unavailable in the isolated test environment; full LSP coverage requires an installed Rust toolchain with rust-analyzer`;
  }
  return null;
}

function hermeticEnv(home, mockUrl) {
  const inherited = Object.fromEntries(Object.entries(process.env).filter(([k]) => !ISOLATED_ENV.test(k) && !ISOLATED_NAMES.has(k)));
  for (const [name, directory] of [['RUSTUP_HOME', '.rustup'], ['CARGO_HOME', '.cargo']]) {
    const installed = path.join(os.homedir(), directory);
    if (inherited[name] === undefined && fs.statSync(installed, { throwIfNoEntry: false })?.isDirectory()) inherited[name] = installed;
  }
  return {
    ...inherited,
    PATH: [path.join(ROOT, 'node_modules', '.bin'), inherited.PATH].filter(Boolean).join(path.delimiter),
    HOME: home, USERPROFILE: home, XDG_CONFIG_HOME: path.join(home, '.config'), GH_CONFIG_DIR: path.join(home, '.gh'),
    GIT_CONFIG_NOSYSTEM: '1', GIT_TERMINAL_PROMPT: '0',
    // Network defaults reject non-loopback traffic; startSurfaces replaces only
    // HTTPS with the allowlisted local registry proxy.
    NO_PROXY: '127.0.0.1,localhost,::1', no_proxy: '127.0.0.1,localhost,::1',
    HTTPS_PROXY: DEAD_PROXY, https_proxy: DEAD_PROXY, HTTP_PROXY: DEAD_PROXY, http_proxy: DEAD_PROXY, ALL_PROXY: DEAD_PROXY, all_proxy: DEAD_PROXY,
    OCTOCODE_HOME: path.join(home, '.octocode'), OCTOCODE_BETA: 'true', OCTOCODE_LSP_AUTO_INSTALL: 'off',
    GITHUB_API_URL: `${mockUrl}/api/v3`, OCTOCODE_ALLOW_PRIVATE_REGISTRY: 'true',
    OCTOCODE_CLASSIFICATION_API_HOST: mockUrl,
  };
}

/** A CLI run as a response record; exit 1 with every row empty/outOfRange is an answer, not an error. */
function cliRecord({ status, stdout, stderr, ms }) {
  let raw;
  try { raw = JSON.parse(stdout); } catch { raw = undefined; }
  const json = raw;
  const emptyAnswer = status === 1 && ['empty', 'outOfRange'].includes(statusClass(raw));
  return { surface: 'cli', status, raw, json, sc: json, text: stdout + stderr, stdout, stderr, responseBytes: Buffer.byteLength(stdout, 'utf8'), isError: !raw || (status !== 0 && !emptyAnswer), ms: Math.round(ms) };
}

async function mcpCall(server, tool, input) {
  const entry = await server.raw(tool, input, '', { keepRaw: true });
  let raw = entry.raw;
  if (!raw) {
    try { raw = JSON.parse(entry.text); } catch { raw = undefined; }
  }
  const json = raw;
  return { surface: 'mcp', isError: entry.isError, raw, json, sc: json, text: entry.text, ms: entry.ms, responseBytes: entry.responseBytes };
}

/**
 * Start the hermetic world over `fixture`: `ctx` ({ mock, fixture, tmp }) is what
 * cases see; `cliArgs`/`cliCall`/`mcpServer`/`mcpCall` run each surface with the
 * tool's own credential; `needs[kind]()` says why a capability is missing (or null).
 */
export async function startSurfaces(fixture) {
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-tool-health-'));
  const home = path.join(tmp, 'home');
  fs.mkdirSync(home, { recursive: true });
  const allTokens = contract.tools.flatMap(t => ['cli', 'mcp'].map(s => tokenFor(t.name, s)));
  const mock = await startMock({ fixtureDir: fixture, tmpDir: tmp, token: allTokens, configureModel: addBulkPullRequest });
  let registry;
  try { registry = await startRegistryProxy(tmp); }
  catch (error) { await mock.close(); fs.rmSync(tmp, { recursive: true, force: true }); throw error; }
  const env = { ...hermeticEnv(home, mock.url), HTTPS_PROXY: registry.proxyUrl, https_proxy: registry.proxyUrl, SSL_CERT_FILE: registry.caFile, SSL_CERT_DIR: '' };
  const envFor = (tool, surface) => ({ ...env, GITHUB_TOKEN: tokenFor(tool, surface), OCTOCODE_CLASSIFICATION_API: tokenFor(tool, surface) });
  const scenario = new AsyncLocalStorage();
  let scenarioIndex = 0;
  /** The CLI, asynchronously so tools run in parallel. */
  const cliArgs = async (argv, { tool = 'schema', cwd = fixture, extraEnv = {} } = {}) => cliRecord(await spawnCli(argv, { cwd, env: { ...envFor(tool, 'cli'), ...(scenario.getStore()?.surface === 'cli' ? scenario.getStore().env : {}), ...extraEnv } }));
  return {
    ctx: { mock, registry, fixture, tmp },
    workspace: name => {
      const cwd = path.join(tmp, `workspace-${name}`);
      fs.cpSync(fixture, cwd, { recursive: true });
      return cwd;
    },
    cliArgs,
    cliCall: (tool, input, { credentialTool = tool, ...opts } = {}) => cliArgs([tool, JSON.stringify(input), '--json'], { tool: credentialTool, ...opts }),
    mcpServer: (tool, cwd) => startServer({ cwd, env: envFor(tool, 'mcp'), replaceEnv: true, timeoutMs: 60_000 }),
    mcpCall: (server, tool, input) => mcpCall(scenario.getStore()?.server ?? server, tool, input),
    scenario: async ({ tool, surface, cwd = fixture, name }, callback) => {
      const scopedEnv = envFor(`${tool}-${name}-${++scenarioIndex}`, surface);
      const server = surface === 'mcp' ? await startServer({ cwd, env: scopedEnv, replaceEnv: true, timeoutMs: 60_000 }) : null;
      try { return await scenario.run({ surface, env: scopedEnv, server }, callback); }
      finally { server?.close(); }
    },
    needs: {
      lsp: () => lspAvailable(fixture, env),
      clone: () => (mock.git ? null : `git smart-HTTP mock unavailable: ${mock.gitUnavailable}`),
    },
    close: async () => { await registry.close(); await mock.close(); fs.rmSync(tmp, { recursive: true, force: true }); },
  };
}
