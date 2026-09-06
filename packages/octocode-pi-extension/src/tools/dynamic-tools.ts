/**
 * dynamic-tools — the deterministic core of the `callTool` meta-tool.
 *
 * A "dynamic tool" is a self-contained, verified capability persisted to a
 * filesystem registry under `getOctocodeHome()/extension/dynamic-tools/`. Each tool is a
 * directory with a `tool.mjs` (default async export `(metadata) => result`), a
 * `tool.test.mjs` used as a verification gate, and an entry in `index.json`.
 *
 * This module owns everything that is deterministic and unit-testable:
 *   - registry read/write (atomic, concurrency-safe via write-temp + rename)
 *   - O(1) resolve (exact name) with a keyword/description fallback
 *   - verification-gated registration (a tool is only indexed if its test passes)
 *   - checksum tamper-detection before execution
 *   - capability gating (declared net/fs/exec must be explicitly allowed)
 *   - isolated subprocess execution with a hard timeout
 *
 * Codegen (create/enhance/fix) is intentionally NOT here — it needs an LLM and
 * lives in the `callTool` tool, which spawns a tool-smith subagent and then calls
 * `registerGeneratedTool` with the produced source + test. Keeping codegen out of
 * this module keeps the core deterministic and fully testable.
 */

import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { getOctocodeHome } from '@octocodeai/config';
import { extensionHome, extensionTmpRoot } from '../extension-paths.js';
import { KEYWORD_MATCH_THRESHOLD, tokenize, withRegistryLock, writeJsonAtomic, readJsonSafe } from './registry-store.js';
import { ensurePrivateDirectory, hardenPrivateFile, PRIVATE_FILE_MODE } from '@octocodeai/octocode-awareness/mcp-state';

/** A capability a dynamic tool may declare. Escalation beyond `[]` needs approval. */
export type Capability = 'net' | 'fs' | 'exec';

export interface ToolManifestEntry {
  name: string;
  description: string;
  keywords: string[];
  capabilities: Capability[];
  /** Why this capability deserves a persisted, reusable tool (recorded at create time). */
  reason: string;
  /**
   * Enforce OS-level isolation when running this tool (Node permission model:
   * denied-by-default fs/net/child_process, scrubbed env). Default true. A tool may be
   * created with `sandboxed:false` only for trusted capabilities that genuinely need
   * broad host access; that decision is recorded here.
   */
  sandboxed: boolean;
  /**
   * The tool is a pure function of its metadata (same input → same output, no side
   * effects). When true AND it declares no capabilities, results are memoized per
   * (name, version, metadata) to skip repeat subprocess spawns. Default false.
   */
  deterministic: boolean;
  /** Absolute path to the tool's `tool.mjs`. */
  entry: string;
  version: number;
  /** sha256 of the exact `tool.mjs` source at registration time. */
  checksum: string;
  createdAt: string;
  updatedAt: string;
  stats: { calls: number; failures: number; lastUsedAt: string | null };
}

interface RegistryIndex {
  version: 1;
  tools: Record<string, ToolManifestEntry>;
}

export type ResolveResult =
  | { hit: 'exact'; entry: ToolManifestEntry }
  | { hit: 'keyword'; entry: ToolManifestEntry; score: number }
  | { hit: 'miss' };

export interface RegisterInput {
  name: string;
  description: string;
  keywords: string[];
  capabilities: Capability[];
  /** Why this tool should exist as a reusable capability (required, non-empty). */
  reason: string;
  /** Enforce OS-level isolation (default true). Set false only for trusted broad-access tools. */
  sandboxed?: boolean;
  /** Pure function of metadata → enables result memoization (default false). */
  deterministic?: boolean;
  /** Full source of `tool.mjs`. */
  source: string;
  /** Full source of `tool.test.mjs` — MUST exit 0 for the tool to be registered. */
  test: string;
}

export type RegisterResult =
  | { ok: true; entry: ToolManifestEntry }
  | { ok: false; reason: 'invalid-name' | 'no-reason' | 'test-failed' | 'test-timeout'; detail?: string };

export type RunResult =
  | { ok: true; result: unknown; cached?: boolean }
  | {
      ok: false;
      reason:
        | 'not-found'
        | 'checksum-mismatch'
        | `capability-denied:${Capability}`
        | 'exec-failed'
        | 'exec-timeout'
        | 'bad-output';
      detail?: string;
    };

// In-memory memoization for deterministic tools. Process-scoped (safe: keyed by tool
// version, so re-registration busts it) and bounded LRU so it can't grow unbounded
// and a hot entry is not evicted before colder recent ones.
const RESULT_CACHE_MAX = 256;
const resultCache = new Map<string, unknown>();
function storeInCache(key: string, value: unknown): void {
  // delete-then-set moves the key to the most-recently-used position.
  resultCache.delete(key);
  resultCache.set(key, value);
  while (resultCache.size > RESULT_CACHE_MAX) {
    const oldest = resultCache.keys().next().value;
    if (oldest === undefined) break;
    resultCache.delete(oldest);
  }
}
/** Read a cached result, refreshing its LRU recency on hit. */
function readFromCache(key: string): unknown {
  const value = resultCache.get(key);
  resultCache.delete(key);
  resultCache.set(key, value);
  return value;
}

const DEFAULT_RUN_TIMEOUT_MS = 5_000;
const DEFAULT_TEST_TIMEOUT_MS = 15_000;
const NAME_RE = /^[a-zA-Z][a-zA-Z0-9_-]{0,63}$/;

// ─── paths ──────────────────────────────────────────────────────────────────

/** Registry root: `getOctocodeHome()/extension/dynamic-tools`. Never hand-roll this path. */
export function getRegistryDir(env?: NodeJS.ProcessEnv): string {
  return path.join(extensionHome(getOctocodeHome(env)), 'dynamic-tools');
}

function indexPath(dir: string): string {
  return path.join(dir, 'index.json');
}

function toolDir(dir: string, name: string): string {
  return path.join(dir, name);
}

// ─── registry io (atomic) ─────────────────────────────────────────────────────

function ensureRegistry(dir: string): void {
  ensurePrivateDirectory(dir);
  if (!fs.existsSync(indexPath(dir))) {
    writeIndex(dir, { version: 1, tools: {} });
  }
}

export function readIndex(dir = getRegistryDir()): RegistryIndex {
  ensureRegistry(dir);
  return readJsonSafe<RegistryIndex>(
    indexPath(dir),
    { version: 1, tools: {} },
    (raw) => Boolean((raw as RegistryIndex).tools),
  );
}

function writeIndex(dir: string, idx: RegistryIndex): void {
  writeJsonAtomic(indexPath(dir), idx);
}

/** Cross-process mutex around a read-modify-write of the shared tools registry. */
function withIndexLock<T>(dir: string, fn: () => T): T {
  return withRegistryLock(dir, '.index.lock', 'dynamic-tools', fn);
}

const sha256 = (s: string): string => crypto.createHash('sha256').update(s).digest('hex');

// ─── resolve (O(1) exact + keyword fallback) ──────────────────────────────────

/**
 * Resolve a request to an existing tool. Exact name match is O(1). On a miss,
 * a keyword overlap against tool keywords (from `toolType` + `intent`) is scored;
 * a candidate needs at least KEYWORD_MATCH_THRESHOLD overlapping tokens to count,
 * which keeps false reuse low. Anything below threshold is a genuine miss.
 */
/** Parse a tool's lastUsedAt into epoch ms (0 when never used / unparseable). */
function lastUsedMs(entry: ToolManifestEntry): number {
  const t = entry.stats?.lastUsedAt ? Date.parse(entry.stats.lastUsedAt) : NaN;
  return Number.isFinite(t) ? t : 0;
}

export function resolveTool(
  toolType: string,
  intent = '',
  dir = getRegistryDir(),
): ResolveResult {
  const idx = readIndex(dir);
  const exact = idx.tools[toolType];
  if (exact) return { hit: 'exact', entry: exact };

  const tokens = tokenize(`${toolType} ${intent}`);
  let best: { entry: ToolManifestEntry; score: number } | null = null;
  for (const entry of Object.values(idx.tools)) {
    const kw = new Set(entry.keywords.map((k) => k.toLowerCase()));
    let score = 0;
    for (const t of tokens) if (kw.has(t)) score++;
    // A candidate must clear its OWN threshold: normally KEYWORD_MATCH_THRESHOLD
    // overlapping tokens, but a tool that declares fewer keywords can never reach
    // that — cap the requirement at its keyword count so single-keyword tools are
    // still resolvable (otherwise they are permanently invisible → registry bloat).
    const need = Math.min(KEYWORD_MATCH_THRESHOLD, Math.max(1, entry.keywords.length));
    if (score < need) continue;
    // Deterministic tie-break: prefer the higher score, then the more recently
    // used tool (Object.values order is otherwise insertion/JSON dependent).
    if (
      !best ||
      score > best.score ||
      (score === best.score && lastUsedMs(entry) > lastUsedMs(best.entry))
    ) {
      best = { entry, score };
    }
  }
  if (best) {
    return { hit: 'keyword', entry: best.entry, score: best.score };
  }
  return { hit: 'miss' };
}



// ─── verification-gated registration ──────────────────────────────────────────

/**
 * Write source + test to the registry and register the tool ONLY if the generated
 * test exits 0. A tool that cannot pass its own test is rejected and leaves no
 * registry entry — no stubs, no half-registered capabilities.
 */
export function registerGeneratedTool(
  input: RegisterInput,
  dir = getRegistryDir(),
  testTimeoutMs = DEFAULT_TEST_TIMEOUT_MS,
): RegisterResult {
  if (!NAME_RE.test(input.name)) {
    return { ok: false, reason: 'invalid-name', detail: input.name };
  }
  if (!input.reason || !input.reason.trim()) {
    return { ok: false, reason: 'no-reason' };
  }
  ensureRegistry(dir);
  const tdir = toolDir(dir, input.name);
  const existing = readIndex(dir).tools[input.name];
  const nextVersion = existing ? existing.version + 1 : 1;
  const entryFile = path.join(tdir, 'tool.mjs');
  const testFile = path.join(tdir, 'tool.test.mjs');

  // Enhance/fix rollback: snapshot the previous good files so a failed regeneration
  // never leaves a soft-broken tool (files changed but index checksum stale).
  const backup = existing ? snapshotFiles(entryFile, testFile) : null;

  ensurePrivateDirectory(tdir);
  fs.writeFileSync(entryFile, input.source, { mode: PRIVATE_FILE_MODE });
  fs.writeFileSync(testFile, input.test, { mode: PRIVATE_FILE_MODE });
  hardenPrivateFile(entryFile);
  hardenPrivateFile(testFile);

  // Sandbox the verification test with the SAME isolation the tool gets at runtime.
  // The test is LLM-authored code; running it with full process.env + unrestricted
  // fs/net/exec would let a benign-looking-but-malicious test exfiltrate secrets or
  // write anywhere while still "passing" the gate. Grant only the tool's declared
  // capabilities plus read/write access to its own tool dir (to import tool.mjs).
  const testRealPath = safeRealpath(testFile);
  const { args: testArgs, env: testEnv } = buildTestInvocation(testRealPath, {
    name: input.name,
    capabilities: input.capabilities,
    sandboxed: input.sandboxed !== false,
  });
  const res = spawnSync(process.execPath, testArgs, {
    encoding: 'utf8',
    timeout: testTimeoutMs,
    env: testEnv,
  });

  if (res.error && (res.error as NodeJS.ErrnoException).code === 'ETIMEDOUT') {
    rollback(tdir, existing, entryFile, testFile, backup);
    return { ok: false, reason: 'test-timeout' };
  }
  if (res.status !== 0) {
    rollback(tdir, existing, entryFile, testFile, backup);
    return {
      ok: false,
      reason: 'test-failed',
      detail: (res.stderr || res.stdout || '').trim().slice(0, 500),
    };
  }

  const now = new Date().toISOString();
  const entry: ToolManifestEntry = {
    name: input.name,
    description: input.description,
    keywords: input.keywords,
    capabilities: input.capabilities,
    reason: input.reason.trim(),
    sandboxed: input.sandboxed !== false,
    deterministic: input.deterministic === true,
    entry: entryFile,
    version: nextVersion,
    checksum: sha256(input.source),
    createdAt: existing?.createdAt ?? now,
    updatedAt: now,
    stats: existing?.stats ?? { calls: 0, failures: 0, lastUsedAt: null },
  };
  withIndexLock(dir, () => {
    const idx = readIndex(dir);
    idx.tools[input.name] = entry;
    writeIndex(dir, idx);
  });
  return { ok: true, entry };
}

interface FileSnapshot {
  source: string | null;
  test: string | null;
}
function snapshotFiles(entryFile: string, testFile: string): FileSnapshot {
  const read = (p: string) => (fs.existsSync(p) ? fs.readFileSync(p, 'utf8') : null);
  return { source: read(entryFile), test: read(testFile) };
}

/**
 * Undo a failed (re)registration.
 *   - brand-new tool (no prior version): remove the directory entirely.
 *   - existing tool (enhance/fix): restore the previous good source + test so the tool
 *     keeps working and its index checksum stays valid — no soft-broken state.
 */
function rollback(
  tdir: string,
  existing: ToolManifestEntry | undefined,
  entryFile: string,
  testFile: string,
  backup: FileSnapshot | null,
): void {
  if (!existing) {
    fs.rmSync(tdir, { recursive: true, force: true });
    return;
  }
  if (backup?.source !== null && backup?.source !== undefined) {
    fs.writeFileSync(entryFile, backup.source, { mode: PRIVATE_FILE_MODE });
    hardenPrivateFile(entryFile);
  }
  if (backup?.test !== null && backup?.test !== undefined) {
    fs.writeFileSync(testFile, backup.test, { mode: PRIVATE_FILE_MODE });
    hardenPrivateFile(testFile);
  }
}

// ─── isolated execution ────────────────────────────────────────────────────────

/**
 * Execute a registered tool in a fresh Node subprocess.
 *   - checksum guard: refuses to run if `tool.mjs` was edited out-of-band
 *   - capability gate: every declared capability must be in `allow`
 *   - hard timeout: a runaway tool is killed
 * The tool receives `metadata` as JSON on stdin and returns its result as JSON stdout.
 */
export function runDynamicTool(
  entry: ToolManifestEntry,
  metadata: unknown,
  opts: { allow?: Capability[]; timeoutMs?: number } = {},
): RunResult {
  const allow = opts.allow ?? [];
  const timeoutMs = opts.timeoutMs ?? DEFAULT_RUN_TIMEOUT_MS;

  if (!fs.existsSync(entry.entry)) return { ok: false, reason: 'not-found' };
  const src = fs.readFileSync(entry.entry, 'utf8');
  if (sha256(src) !== entry.checksum) return { ok: false, reason: 'checksum-mismatch' };

  for (const cap of entry.capabilities) {
    if (!allow.includes(cap)) return { ok: false, reason: `capability-denied:${cap}` };
  }

  // Memoize pure tools: same (name, version, metadata) → skip the subprocess. Only when
  // the tool declares determinism AND no capabilities (a tool with net/fs/exec may have
  // side effects, so caching would be unsafe). Keyed by version so re-registration busts it.
  const cacheable = entry.deterministic && entry.capabilities.length === 0;
  const cacheKey = cacheable
    ? `${entry.name}@${entry.version}:${sha256(JSON.stringify(metadata ?? {}))}`
    : '';
  if (cacheable && resultCache.has(cacheKey)) {
    return { ok: true, result: readFromCache(cacheKey), cached: true };
  }

  // Isolate the runner in its own mkdtemp dir so the sandbox fs-read grant
  // (readSubtree → <dir>/*) scopes to exactly this runner, not the whole OS
  // temp dir (which would expose sibling temp files: other runners, editor swap
  // files, downloaded tarballs, etc.).
  const runnerRoot = path.join(extensionTmpRoot(), 'calltool');
  ensurePrivateDirectory(runnerRoot);
  const runnerDir = fs.mkdtempSync(path.join(runnerRoot, 'runner-'));
  ensurePrivateDirectory(runnerDir);
  const runner = path.join(
    runnerDir,
    `runner-${process.pid}-${Date.now()}-${Math.random().toString(36).slice(2)}.mjs`,
  );
  // Import via the tool's realpath so the sandboxed permission check (which resolves
  // realpaths) matches the granted realpath subtree.
  const toolRealPath = safeRealpath(entry.entry);
  fs.writeFileSync(
    runner,
    [
      // Read metadata from stdin (not argv) so large inputs never hit the OS argv limit.
      `let _in = ''; for await (const c of process.stdin) _in += c;`,
      `const metadata = JSON.parse(_in || '{}');`,
      `const mod = await import(${JSON.stringify(pathToFileUrl(toolRealPath))});`,
      `if (typeof mod.default !== 'function') { console.error('tool has no default export function'); process.exit(3); }`,
      `const out = await mod.default(metadata);`,
      `process.stdout.write(JSON.stringify(out ?? null));`,
    ].join('\n'),
    { mode: PRIVATE_FILE_MODE },
  );

  // Pass the runner's realpath as the entry so Node does not need to read the tmp
  // symlink chain (e.g. /var → /private/var) during main-entry realpath resolution.
  const runnerRealPath = safeRealpath(runner);
  const { args, env } = buildRunInvocation(runnerRealPath, toolRealPath, entry, allow);

  try {
    const res = spawnSync(process.execPath, args, {
      encoding: 'utf8',
      timeout: timeoutMs,
      env,
      input: JSON.stringify(metadata ?? {}),
    });
    if (res.error && (res.error as NodeJS.ErrnoException).code === 'ETIMEDOUT') {
      return { ok: false, reason: 'exec-timeout' };
    }
    if (res.status !== 0) {
      return { ok: false, reason: 'exec-failed', detail: (res.stderr || '').trim().slice(0, 500) };
    }
    try {
      const parsed = JSON.parse(res.stdout);
      if (cacheable) storeInCache(cacheKey, parsed);
      return { ok: true, result: parsed, cached: false };
    } catch {
      return { ok: false, reason: 'bad-output', detail: res.stdout.slice(0, 300) };
    }
  } finally {
    fs.rmSync(runnerDir, { recursive: true, force: true });
  }
}

/** Resolve a path's realpath, falling back to the resolved absolute path. */
function safeRealpath(file: string): string {
  try {
    return fs.realpathSync(file);
  } catch {
    return path.resolve(file);
  }
}

/** A realpath'd file into a Node permission subtree glob (`/real/dir/*`). */
function readSubtree(realFile: string): string {
  return `${path.dirname(realFile)}${path.sep}*`;
}

/**
 * Build the sandboxed Node invocation for a generated tool's verification TEST.
 * Mirrors buildRunInvocation but scopes fs access to the tool's own directory
 * (where tool.mjs + tool.test.mjs live) so the test can import the module, and
 * grants exactly the capabilities the tool declares. Env is scrubbed to PATH.
 */
function buildTestInvocation(
  testRealPath: string,
  entry: { name: string; capabilities: Capability[]; sandboxed: boolean },
): { args: string[]; env: NodeJS.ProcessEnv } {
  return buildSandboxedNodeInvocation({
    entryFile: testRealPath,
    sandboxed: entry.sandboxed,
    capabilities: entry.capabilities,
    allowedCapabilities: entry.capabilities,
    readScopes: [testRealPath],
    writeScopes: [testRealPath],
  });
}

/**
 * Build the Node invocation + environment for a tool run.
 *
 * Sandboxed (default): enable the Node permission model so everything is denied by
 * default, then grant ONLY the capabilities the tool declared and the caller approved:
 *   - fs-read is scoped to the runner + tool directories (needed to import the module)
 *   - `net`  → --allow-net
 *   - `exec` → --allow-child-process
 *   - `fs`   → --allow-fs-read + --allow-fs-write (broad; the fs capability IS the approval)
 * Native addons, workers, FFI, and the inspector are never granted. The environment is
 * scrubbed to a minimal PATH so process secrets in process.env are not exposed.
 *
 * Non-sandboxed (opt-in, trusted tools only): run as an ordinary Node process with the
 * inherited environment — capabilities are advisory, used only for the approval gate.
 */
function buildRunInvocation(
  runnerRealPath: string,
  toolRealPath: string,
  entry: ToolManifestEntry,
  allow: Capability[],
): { args: string[]; env: NodeJS.ProcessEnv } {
  return buildSandboxedNodeInvocation({
    entryFile: runnerRealPath,
    sandboxed: entry.sandboxed,
    capabilities: entry.capabilities,
    allowedCapabilities: allow,
    readScopes: [runnerRealPath, toolRealPath],
    writeScopes: [],
  });
}

interface SandboxInvocationPolicy {
  entryFile: string;
  sandboxed: boolean;
  capabilities: Capability[];
  allowedCapabilities: Capability[];
  readScopes: string[];
  writeScopes: string[];
}

function buildSandboxedNodeInvocation(policy: SandboxInvocationPolicy): { args: string[]; env: NodeJS.ProcessEnv } {
  if (!policy.sandboxed) {
    return { args: [policy.entryFile], env: process.env };
  }

  const caps = new Set(policy.capabilities.filter((c) => policy.allowedCapabilities.includes(c)));
  // --disallow-code-generation-from-strings blocks eval / new Function in generated code,
  // shrinking the attack surface beyond the explicit capability grants.
  const flags = ['--permission', '--disallow-code-generation-from-strings'];

  if (caps.has('fs')) {
    flags.push('--allow-fs-read=*', '--allow-fs-write=*');
  } else {
    for (const scope of policy.readScopes) flags.push(`--allow-fs-read=${readSubtree(scope)}`);
    for (const scope of policy.writeScopes) flags.push(`--allow-fs-write=${readSubtree(scope)}`);
  }
  if (caps.has('net')) flags.push('--allow-net');
  if (caps.has('exec')) flags.push('--allow-child-process');

  // Scrub the environment: expose only PATH so explicitly approved exec tools can
  // locate binaries, but never inherit API keys / tokens from process.env.
  const env: NodeJS.ProcessEnv = { PATH: process.env.PATH ?? '' };
  return { args: [...flags, policy.entryFile], env };
}

function pathToFileUrl(p: string): string {
  const resolved = path.resolve(p);
  const prefixed = resolved.startsWith('/') ? resolved : `/${resolved.replace(/\\/g, '/')}`;
  return `file://${encodeURI(prefixed)}`;
}

// ─── listing / stats ────────────────────────────────────────────────────────

export function listTools(dir = getRegistryDir()): ToolManifestEntry[] {
  return Object.values(readIndex(dir).tools);
}

/** Delete a dynamic tool: remove its directory and index entry (CRUD delete). */
export function deleteTool(name: string, dir = getRegistryDir()): boolean {
  return withIndexLock(dir, () => {
    const idx = readIndex(dir);
    if (!idx.tools[name]) return false;
    delete idx.tools[name];
    writeIndex(dir, idx);
    fs.rmSync(toolDir(dir, name), { recursive: true, force: true });
    return true;
  });
}

/** Record a call outcome for a tool (drives enhance/optimize + GC decisions). */
export function recordUsage(name: string, ok: boolean, dir = getRegistryDir()): void {
  withIndexLock(dir, () => {
    const idx = readIndex(dir);
    const entry = idx.tools[name];
    if (!entry) return;
    entry.stats.calls += 1;
    if (!ok) entry.stats.failures += 1;
    entry.stats.lastUsedAt = new Date().toISOString();
    writeIndex(dir, idx);
  });
}
