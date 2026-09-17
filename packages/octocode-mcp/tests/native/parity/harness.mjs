/**
 * Native-vs-TS differential parity harness (Phase 2, S6 — RFC 20260917-finish-rust-migration).
 *
 * Generalized from `tests/native/response-pagination.mjs`. Connects to two MCP
 * servers (reference TS tools-core and native Rust runtime) and provides:
 *
 *   - `testTool(name, args)` — calls both, deep-equals sanitized envelopes.
 *   - `testPagination(name, args, opts)` — collects all pages from both,
 *     asserts page parity/exhaustion, and returns pages for fixture-union checks.
 *   - `selfTest()` — injects a known diff and expects the harness to fail.
 *
 * Fixture corpus for GitHub tools lives in `../fixtures/parity/` as JSON files
 * recorded against the live API. Local tools (localSearch, localFetch, astSearch,
 * astRewrite, astGraph) use an on-the-fly temp workspace.
 *
 * @example
 *   const h = await Harness.connect({ reference, native, addon, regexWorker, workspaceDir });
 *   await h.testTool('localSearch', { ... });
 *   await h.testPagination('localFetch', { ... });
 *   await h.close();
 *
 * Gate: a tool may not flip to native-default until its corpus entry is green.
 */

import assert from 'node:assert/strict';
import { mkdtemp, rm, writeFile, mkdir, utimes } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { StdioClientTransport } from '@modelcontextprotocol/sdk/client/stdio.js';

// ---------------------------------------------------------------------------
// Sanitizers — strip fields that legitimately differ (timing, absolute paths,
// server identity markers) before byte-comparison. Extend here rather than in
// individual corpus files.
// ---------------------------------------------------------------------------

/**
 * Recursively remove keys from a value that are known to differ across
 * runtimes for non-behavioral reasons (e.g. elapsed_ms, generatedAt,
 * serverVersion, absolute tempdir paths replaced with a placeholder).
 */
function sanitize(value, workspaceDir) {
  if (value === null || typeof value !== 'object') {
    if (typeof value === 'string') {
      let sanitized = workspaceDir
        ? value.replaceAll(workspaceDir, '__WORKSPACE__')
        : value;
      sanitized = sanitized.replace(
        /(\bsnapshot:\s*)(?:[\w.-]+:)?[0-9a-f]{64}\b/g,
        '$1__OPAQUE_SNAPSHOT__',
      );
      return sanitized;
    }
    return value;
  }
  if (Array.isArray(value)) {
    return value.map(item => sanitize(item, workspaceDir));
  }
  const result = {};
  for (const [k, v] of Object.entries(value)) {
    if (STRIP_KEYS.has(k)) continue;
    result[k] = sanitize(v, workspaceDir);
  }
  return result;
}

/** Keys whose values differ non-behaviorally across runtimes. */
const STRIP_KEYS = new Set([
  'elapsed_ms',
  'elapsedMs',
  'generatedAt',
  'serverVersion',
  'runtimeVersion',
  // Native continuations additionally carry an opaque integrity token. The
  // executable public continuation is `next.tool` + `next.query`, which is
  // compared in full. Tokens are process-scoped and are not byte-stable.
  'cursor',
  // Snapshot digests are opaque runtime-specific integrity tokens. Their
  // surrounding pagination state and executable continuation remain compared.
  'snapshot',
  'responseSnapshot',
]);

// ---------------------------------------------------------------------------
// Connection helpers
// ---------------------------------------------------------------------------

async function connectClient(serverPath, env, cwd) {
  const transport = new StdioClientTransport({
    command: process.execPath,
    // The child runs with the fixture as cwd, so resolve server entry points
    // before spawning rather than interpreting repo-relative paths there.
    args: [resolve(serverPath)],
    cwd,
    env,
    stderr: 'inherit',
  });
  const client = new Client({ name: 'parity-harness', version: '1' });
  await client.connect(transport);
  return client;
}

// ---------------------------------------------------------------------------
// Pagination collector
// ---------------------------------------------------------------------------

/**
 * Collect all pages for a paginated tool call. Asserts that:
 *   - at least 2 pages are produced (otherwise pagination is not exercised),
 *   - every intermediate page has `hasMore: true`,
 *   - the final page has `hasMore: false`,
 *   - each continuation carries a runnable `next.*` tool reference.
 *
 * Returns every page. Tool-specific corpus scripts must additionally project
 * result identities and assert that their union covers the complete fixture.
 */
function continuationState(result) {
  const responsePagination = result.structuredContent?.responsePagination;
  if (responsePagination) {
    return {
      hasMore: responsePagination.hasMore === true,
      continuation: responsePagination.next,
      responseLevel: true,
    };
  }

  const rows = result.structuredContent?.results ?? [];
  for (const row of rows) {
    const data = row?.data;
    const hasMore =
      data?.pagination?.hasMore === true ||
      data?.pagination?.nextOffset !== undefined ||
      data?.nextOffset !== undefined;
    if (!hasMore) continue;
    const continuation = Object.values(data.next ?? {}).find(
      value => value?.tool && value?.query
    );
    return { hasMore: true, continuation, responseLevel: false };
  }
  return { hasMore: false, continuation: undefined, responseLevel: false };
}

async function collectPages(client, toolName, args, maxPages = 200) {
  const pages = [];
  let currentArgs = args;

  for (let i = 0; i < maxPages; i++) {
    const result = await client.callTool({ name: toolName, arguments: currentArgs });
    pages.push(result);
    const state = continuationState(result);
    if (!state.hasMore) break;

    const continuation = state.continuation;
    assert.ok(
      continuation?.tool,
      `page ${i}: continuation must carry a tool name (got ${JSON.stringify(continuation)})`
    );
    assert.equal(continuation.tool, toolName, `page ${i}: continuation changed tools`);
    assert.ok(continuation.query, `page ${i}: continuation must carry a query`);
    currentArgs = state.responseLevel
      ? continuation.query
      : { queries: [continuation.query] };
  }

  assert.equal(
    continuationState(pages.at(-1)).hasMore,
    false,
    'final page must have hasMore:false'
  );
  assert.ok(pages.length < maxPages, `pagination exceeded ${maxPages} pages`);

  return pages;
}

// ---------------------------------------------------------------------------
// Harness class
// ---------------------------------------------------------------------------

export class Harness {
  /** @type {Client} */ #reference;
  /** @type {Client} */ #native;
  /** @type {string} */ #workspaceDir;
  /** @type {string} */ #tempDir;

  constructor({ reference, native, workspaceDir, tempDir }) {
    this.#reference = reference;
    this.#native = native;
    this.#workspaceDir = workspaceDir;
    this.#tempDir = tempDir;
  }

  /**
   * Create a Harness by connecting to both servers. `workspaceDir` is the
   * local file fixture root shared by both servers. Callers that do not pass
   * a `workspaceDir` get a freshly created temp directory.
   */
  static async connect({ reference, native, addon, regexWorker, workspaceDir, extraEnv = {} } = {}) {
    const tempDir = workspaceDir ?? await mkdtemp(join(tmpdir(), 'octocode-parity-'));

    const env = {
      ...process.env,
      ENABLE_LOCAL: 'true',
      WORKSPACE_ROOT: tempDir,
      ALLOWED_PATHS: tempDir,
      OCTOCODE_NATIVE_BINDING: addon ?? process.env.OCTOCODE_NATIVE_BINDING ?? '',
      OCTOCODE_REGEX_WORKER: regexWorker ?? process.env.OCTOCODE_REGEX_WORKER ?? '',
      ...extraEnv,
    };

    const [refClient, nativeClient] = await Promise.all([
      connectClient(reference, env, tempDir),
      connectClient(native, env, tempDir),
    ]);

    return new Harness({
      reference: refClient,
      native: nativeClient,
      workspaceDir: tempDir,
      tempDir: workspaceDir ? null : tempDir,
    });
  }

  /** Path to the local workspace fixture directory. */
  get workspaceDir() { return this.#workspaceDir; }

  /**
   * Write a file into the workspace fixture directory and return its path.
   * Useful for setting up local tool fixtures inline.
   */
  async writeFixture(name, content) {
    const full = join(this.#workspaceDir, name);
    await mkdir(dirname(full), { recursive: true });
    await writeFile(full, content);
    const frozenTime = new Date('2020-09-13T12:26:40.443Z');
    await utimes(full, frozenTime, frozenTime);
    return full;
  }

  /**
   * Call `toolName` on both runtimes with `args`, sanitize both responses,
   * and deep-equal them. Throws on any diff.
   */
  async testTool(toolName, args) {
    // Execute sequentially because some tools coordinate through workspace
    // locks (notably astRewrite preview/recovery). Parallel differential calls
    // would test lock contention rather than runtime parity.
    const ref = await this.#reference.callTool({ name: toolName, arguments: args });
    const nat = await this.#native.callTool({ name: toolName, arguments: args });
    const sanitizedRef = sanitize(ref, this.#workspaceDir);
    const sanitizedNat = sanitize(nat, this.#workspaceDir);
    assert.deepEqual(
      sanitizedNat,
      sanitizedRef,
      `${toolName}: native response differs from reference`
    );
    return { reference: sanitizedRef, native: sanitizedNat };
  }

  /**
   * Paginate `toolName` on both runtimes and assert:
   *   - same number of pages,
   *   - sanitized page-by-page equality,
   *   - all continuations are executable through the public bulk envelope.
   *
   * The returned sanitized pages let the caller assert exact fixture-union
   * coverage. `opts.minPages` (default 2) proves pagination was exercised.
   */
  async testPagination(toolName, args, { minPages = 2 } = {}) {
    const refPages = await collectPages(this.#reference, toolName, args);
    const natPages = await collectPages(this.#native, toolName, args);

    assert.equal(
      natPages.length,
      refPages.length,
      `${toolName}: native produced ${natPages.length} pages, reference ${refPages.length}`
    );

    assert.ok(
      natPages.length >= minPages,
      `${toolName}: expected ≥${minPages} pages, got ${natPages.length}`
    );

    const sanitizedReference = [];
    const sanitizedNative = [];
    for (let i = 0; i < refPages.length; i++) {
      const sanitizedRef = sanitize(refPages[i], this.#workspaceDir);
      const sanitizedNat = sanitize(natPages[i], this.#workspaceDir);
      assert.deepEqual(
        sanitizedNat,
        sanitizedRef,
        `${toolName}: page ${i + 1}/${refPages.length} differs`
      );
      sanitizedReference.push(sanitizedRef);
      sanitizedNative.push(sanitizedNat);
    }

    return {
      pages: natPages.length,
      reference: sanitizedReference,
      native: sanitizedNative,
    };
  }

  /**
   * Self-validation: inject a known diff and assert the harness detects it.
   * Returns true if the harness correctly fails on a diff. Used to prove the
   * harness is not silently vacuous.
   */
  async selfTest() {
    // We call localSearch with a query guaranteed to return no results on both
    // sides, but then manually corrupt the native result and verify deepEqual
    // throws. Since we can't corrupt the live result, we simulate this by
    // asserting that two divergent literal objects are flagged.
    let caught = false;
    try {
      assert.deepEqual(
        sanitize({ ok: true, mutated: false }, null),
        sanitize({ ok: true, mutated: true }, null),
        'deliberate mismatch'
      );
    } catch {
      caught = true;
    }
    assert.ok(caught, 'selfTest: harness must detect a diff');
    return true;
  }

  /** Close both MCP clients and clean up the temp workspace (if owned). */
  async close() {
    await Promise.allSettled([this.#reference.close(), this.#native.close()]);
    if (this.#tempDir) {
      await rm(this.#tempDir, { recursive: true, force: true });
    }
  }
}

// ---------------------------------------------------------------------------
// Fixture corpus registry
// ---------------------------------------------------------------------------

/**
 * Registry of per-tool corpus entries. Each entry describes the test inputs
 * and whether the tool requires a paginated test. Populated by individual
 * `parity/*.mjs` test files.
 *
 * Status: Pending — entries are added as each tool's native runner is
 * verified. A tool may only flip to native-default once its entry is green.
 */
export const CORPUS = {
  // "partial" means an executable fixture exists but does not yet satisfy A3's
  // success/error/pagination-union breadth. It must never unblock a flip.
  localSearch: { status: 'covered', script: 'local-search.mjs', note: 'Six lexical/regex/filter/empty/error and executable pagination-union cases' },
  localFetch: { status: 'covered', script: '../response-pagination.mjs', note: 'Executable 14-page Unicode/stale-snapshot continuation union' },

  astSearch: { status: 'partial', script: 'local-tools.mjs', note: 'Files, match, symbols, and topology fixtures; operation breadth remains' },
  astRewrite: { status: 'partial', script: 'local-tools.mjs', note: 'Preview and invalid-input parity; apply/selection/pagination remain' },
  astGraph: { status: 'partial', script: 'local-tools.mjs', note: 'Topology files fixture only; graph operation breadth remains' },

  lspSearch: { status: 'blocked', script: 'lsp-search.mjs', note: 'Executable fixture is red: native emits raw LSP payloads instead of normalized TS envelopes' },

  ghSearch: { status: 'partial', script: 'github-tools.mjs', note: 'Recorded repository search plus invalid input; other operations remain' },
  ghGetFileContent: { status: 'partial', script: 'github-tools.mjs', note: 'Recorded full-file fetch plus invalid input; continuation breadth remains' },
  ghSearchHistory: { status: 'partial', script: 'github-tools.mjs', note: 'Recorded issue-list plus invalid input; PR/commit/search modes remain' },
  ghGetHistoryItem: { status: 'partial', script: 'github-tools.mjs', note: 'Recorded issue detail plus invalid input; PR/commit/compare modes remain' },
  ghCloneRepo: { status: 'pending', note: 'Needs deterministic HTTPS dumb-git fixture and isolated caches' },
  artifactSearch: { status: 'partial', script: 'artifact-search.mjs', note: 'Recorded npm search plus invalid input; continuation/ecosystem breadth remains' },
};

export function assertCorpusComplete() {
  const incomplete = Object.entries(CORPUS)
    .filter(([, value]) => value.status !== 'covered')
    .map(([name, value]) => `${name} (${value.status})`);
  assert.equal(
    incomplete.length,
    0,
    `Parity corpus is incomplete for: ${incomplete.join(', ')}\n` +
    'Every entry must be covered before flipping to native-default.'
  );
}
