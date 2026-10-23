import fs from 'node:fs';
import path from 'node:path';
import { afterEach, beforeEach, vi } from 'vitest';
import { SessionManager, type SessionInfo } from '@earendil-works/pi-coding-agent';
import { closeSharedAgentDb, sharedAgentDb } from '../src/agentdb/db.js';
import { setCurrentSession } from '../src/shared/home.js';
import { Subcommands } from '../src/shared/commands.js';
import { registerSessions } from '../src/sessions/register.js';
import { SessionIndex } from '../src/sessions/store.js';
import { fakeCtx, fakePi } from './fake-pi.js';
import { tmp } from './helpers.js';

export const HOUR = 3_600_000;
export const DAY = 24 * HOUR;

let saved: { home?: string; db?: string };
let home: string;
/** The per-test workspace; a live binding, reassigned before each test. */
export let cwd: string;
/** What Pi's own session listing returns; a live binding, reset before each test and replaced with `setPiSessions`. */
export let piSessions: SessionInfo[] = [];
export const setPiSessions = (sessions: SessionInfo[]) => (piSessions = sessions);

/** Gives each test of the calling file a fresh home, workspace and agent db, and a Pi session listing it controls. */
export function useSessionsFixture(): void {
  beforeEach(() => {
    saved = { home: process.env['OCTOCODE_HOME'], db: process.env['OCTOCODE_AGENT_DB'] };
    home = tmp();
    cwd = tmp();
    process.env['OCTOCODE_HOME'] = home;
    delete process.env['OCTOCODE_AGENT_DB'];
    // Pi's own session listing would read the real ~/.pi: tests give it per case.
    piSessions = [];
    vi.spyOn(SessionManager, 'list').mockImplementation(async () => piSessions);
    vi.spyOn(SessionManager, 'listAll').mockImplementation(async () => piSessions);
  });

  afterEach(() => {
    vi.restoreAllMocks();
    closeSharedAgentDb();
    setCurrentSession(undefined);
    for (const [key, value] of [['OCTOCODE_HOME', saved.home], ['OCTOCODE_AGENT_DB', saved.db]] as const) {
      if (value === undefined) delete process.env[key];
      else process.env[key] = value;
    }
  });
}

export type Entry = { type: string; id: string; timestamp?: string; message?: Record<string, unknown>; customType?: string; details?: unknown };

/** A Pi host with the sessions feature and a session the test can shape. */
export function setup(options: { id?: string; entries?: Entry[]; hasUI?: boolean; name?: string; subagent?: boolean} = {}) {
  const fake = fakePi();
  const names: string[] = [];
  const switched: string[] = [];
  Object.assign(fake.pi, { setSessionName: (name: string) => names.push(name), getSessionName: () => names.at(-1) ?? options.name });
  const commands = new Subcommands();
  registerSessions(fake.pi, { commands, isSubagent: options.subagent ?? false, filesChanged: () => 3 });
  const id = options.id ?? 'sess-1';
  const file = path.join(tmp(), `${id}.jsonl`);
  fs.writeFileSync(file, '{}\n');
  const entries = options.entries ?? [];
  const ctx = fakeCtx({ cwd, hasUI: options.hasUI ?? true });
  Object.assign(ctx, {
    sessionManager: {
      getSessionId: () => id,
      getSessionFile: () => file,
      getSessionDir: () => path.dirname(file),
      getSessionName: () => options.name,
      getEntries: () => entries,
      getBranch: () => entries,
    },
    switchSession: async (target: string) => (switched.push(target), { cancelled: false }),
  });
  const start = async (reason = 'startup') => {
    setCurrentSession(id);
    await fake.fire('session_start', { type: 'session_start', reason }, ctx);
  };
  return { ...fake, piCommands: fake.commands, commands, ctx, id, file, entries, names, switched, start, index: () => new SessionIndex(sharedAgentDb()) };
}

export const row = (id: string) => new SessionIndex(sharedAgentDb()).get(id);
export const ago = (ms: number) => new Date(Date.now() - ms).toISOString();

