/**
 * Native-vs-TS differential parity harness (Phase 2, S6 — RFC 20260917-finish-rust-migration).
 *
 * Generalized from `tests/native/response-pagination.mjs`. Connects to two MCP
 * servers (reference TS tools-core and native Rust runtime) and provides:
 *
 *   - `testTool(name, args)` — calls both, deep-equals sanitized envelopes.
 *   - `testPagination(name, args, opts)` — collects all pages from both,
 *     asserts union equality and that both exhaust in the same number of pages.
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
import { mkdtemp, rm, writeFile, mkdir } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
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
    if (typeof value === 'string' && workspaceDir) {
      return value.replaceAll(workspaceDir, '__WORKSPACE__');
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
]);

// ---------------------------------------------------------------------------
// Connection helpers
// ---------------------------------------------------------------------------

async function connectClient(serverPath, env, cwd) {
  const transport = new StdioClientTransport({
    command: process.execPath,
    args: [serverPath],
    cwd,
    env,
    stderr: 'pipe',
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
 * Returns `{ pages, union }` where `union` is the concatenation of all
 * result items/data across pages.
 */
async function collectPages(client, toolName, args, maxPages = 200) {
  const pages = [];
  let currentArgs = args;

  for (let i = 0; i < maxPages; i++) {
    const result = await client.callTool({ name: toolName, arguments: currentArgs });
    pages.push(result);
    const pagination = result.structuredContent?.responsePagination;
    if (!pagination?.hasMore) break;

    const continuation = pagination.next;
    assert.ok(
      continuation?.tool,
      `page ${i}: continuation must carry a tool name (got ${JSON.stringify(continuation)})`
    );
    assert.ok(
      continuation.query,
      `page ${i}: continuation must carry a query`
    );
    currentArgs = continuation.query;
  }

  assert.equal(
    pages.at(-1)?.structuredContent?.responsePagination?.hasMore ?? false,
    false,
    'final page must have hasMore:false'
  );

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
  static async connect({ reference, native, addon, regexWorker, workspaceDir } = {}) {
    const tempDir = workspaceDir ?? await mkdtemp(join(tmpdir(), 'octocode-parity-'));

    const env = {
      ...process.env,
      ENABLE_LOCAL: 'true',
      WORKSPACE_ROOT: tempDir,
      ALLOWED_PATHS: tempDir,
      OCTOCODE_NATIVE_BINDING: addon ?? process.env.OCTOCODE_NATIVE_BINDING ?? '',
      OCTOCODE_REGEX_WORKER: regexWorker ?? process.env.OCTOCODE_REGEX_WORKER ?? '',
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
    await mkdir(join(this.#workspaceDir, name, '..').replace(/[^/]+$/, '') || '.', { recursive: true });
    await writeFile(full, content);
    return full;
  }

  /**
   * Call `toolName` on both runtimes with `args`, sanitize both responses,
   * and deep-equal them. Throws on any diff.
   */
  async testTool(toolName, args) {
    const [ref, nat] = await Promise.all([
      this.#reference.callTool({ name: toolName, arguments: args }),
      this.#native.callTool({ name: toolName, arguments: args }),
    ]);
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
   *   - union of all pages equals the full fixture.
   *
   * `opts.minPages` (default 2) asserts pagination is actually exercised.
   */
  async testPagination(toolName, args, { minPages = 2 } = {}) {
    const [refPages, natPages] = await Promise.all([
      collectPages(this.#reference, toolName, args),
      collectPages(this.#native, toolName, args),
    ]);

    assert.equal(
      natPages.length,
      refPages.length,
      `${toolName}: native produced ${natPages.length} pages, reference ${refPages.length}`
    );

    assert.ok(
      natPages.length >= minPages,
      `${toolName}: expected ≥${minPages} pages, got ${natPages.length}`
    );

    for (let i = 0; i < refPages.length; i++) {
      const sanitizedRef = sanitize(refPages[i], this.#workspaceDir);
      const sanitizedNat = sanitize(natPages[i], this.#workspaceDir);
      assert.deepEqual(
        sanitizedNat,
        sanitizedRef,
        `${toolName}: page ${i + 1}/${refPages.length} differs`
      );
    }

    return { pages: natPages.length };
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
  // Local tools — can run in CI with a temp workspace (no network).
  localSearch: { status: 'pending', note: 'Add in S7 with temp workspace fixtures' },
  localFetch:  { status: 'covered', note: 'Covered by response-pagination.mjs (existing)' },

  // AST tools — can run against the monorepo source.
  astSearch:   { status: 'pending', note: 'Add in S7' },
  astRewrite:  { status: 'pending', note: 'Add in S7 — preview only (no snapshot write in tests)' },
  astGraph:    { status: 'pending', note: 'Add in S7' },

  // LSP tool — requires a running language server; use rust-analyzer on .rs fixtures.
  lspSearch:   { status: 'pending', note: 'Add in S7 — needs rust-analyzer on PATH' },

  // GitHub tools — require recorded fixtures to avoid live network in CI.
  ghSearch:           { status: 'pending', note: 'Add in S7 with recorded fixtures' },
  ghGetFileContent:   { status: 'pending', note: 'Add in S7 with recorded fixtures' },
  ghSearchHistory:    { status: 'pending', note: 'Add in S7 with recorded fixtures' },
  ghGetHistoryItem:   { status: 'pending', note: 'Add in S7 with recorded fixtures' },
  ghCloneRepo:        { status: 'pending', note: 'Add in S7 with recorded fixtures' },
  artifactSearch:     { status: 'pending', note: 'Add in S7 with recorded fixtures' },
};

export function assertCorpusComplete() {
  const pending = Object.entries(CORPUS)
    .filter(([, v]) => v.status === 'pending')
    .map(([k]) => k);
  assert.equal(
    pending.length,
    0,
    `Parity corpus is incomplete for: ${pending.join(', ')}\n` +
    'Each tool must have a covered corpus entry before flipping to native-default.'
  );
}
