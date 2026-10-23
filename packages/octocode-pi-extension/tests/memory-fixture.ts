import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { afterEach, beforeEach, vi } from 'vitest';
import { closeSharedAgentDb, openAgentDb, type AgentDb } from '../src/agentdb/db.js';
import type { MemoryInput, MemoryStore } from '../src/memory/store.js';
import { fakeCtx } from './fake-pi.js';

/** The per-test temporary root and its workspace; live bindings, reassigned before each test. */
export let root: string;
export let cwd: string;
const opened: AgentDb[] = [];

/** Gives each test of the calling file a fresh root, home and agent db, and cleans them up after it. */
export function useMemoryFixture(): void {
  beforeEach(() => {
    root = fs.mkdtempSync(path.join(os.tmpdir(), 'octo-memory-'));
    cwd = path.join(root, 'repo');
    fs.mkdirSync(cwd);
    vi.stubEnv('OCTOCODE_HOME', path.join(root, 'home'));
    vi.stubEnv('OCTOCODE_AGENT_DB', '');
    vi.stubEnv('OCTOCODE_MEMORY_AUTO', '');
  });

  afterEach(() => {
    for (const db of opened.splice(0)) db.close();
    closeSharedAgentDb();
    vi.unstubAllEnvs();
    fs.rmSync(root, { recursive: true, force: true });
  });
}

export function freshDb(options: { fts?: boolean } = {}): AgentDb {
  const db = openAgentDb(path.join(root, `db-${opened.length}.db`));
  opened.push(db);
  return options.fts === false ? { ...db, fts: false, transaction: db.transaction.bind(db) } : db;
}

export const add = (store: MemoryStore, input: MemoryInput, now?: number) => store.set({ author: 'agent', ...input }, now).memory;

/** A ctx with the bits the memory feature reads beyond fakeCtx: trust, the branch and the editor. */
export function ctxFor(options: { branch?: unknown[]; trusted?: boolean; hasUI?: boolean; selects?: Array<string | undefined>; confirms?: boolean[]; editor?: Array<string | undefined> } = {}) {
  const ctx = fakeCtx({ cwd, hasUI: options.hasUI ?? true, ui: { selects: options.selects ?? [], confirms: options.confirms ?? [] } });
  const branch = options.branch ?? [];
  const edits = options.editor ?? [];
  const editorCalls: Array<{ title: string; prefill?: string }> = [];
  Object.assign(ctx, { isProjectTrusted: () => options.trusted ?? true });
  Object.assign(ctx.sessionManager, { getBranch: () => branch });
  Object.assign(ctx.ui, {
    editor: async (title: string, prefill?: string) => {
      editorCalls.push({ title, prefill });
      return edits.shift();
    },
  });
  return { ctx, branch, editorCalls };
}

/** Distinct words, so generated fixtures are not near-duplicates of each other. */
export const WORDS = 'alpha bravo charlie delta echo foxtrot golf hotel india juliet kilo lima mike november oscar papa quebec romeo sierra tango uniform victor whiskey xray yankee zulu amber basil cedar dahlia ember fennel garnet hazel iris jasper kelp lotus maple nutmeg olive pepper quartz rowan saffron thyme'.split(' ');
