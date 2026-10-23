import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import type { AgentToolResult } from '@earendil-works/pi-coding-agent';
import { afterEach, beforeEach } from 'vitest';
import { closeSharedAgentDb, sharedAgentDb, type AgentDb } from '../src/agentdb/db.js';
import { registerBacklog } from '../src/backlog/index.js';
import { BacklogStore } from '../src/backlog/store.js';
import { Subcommands } from '../src/shared/commands.js';
import { PARENT_SESSION_ENV } from '../src/shared/env.js';
import { textOf } from './doubles.js';
import { fakeCtx, fakePi } from './fake-pi.js';

export const KEY = { enter: '\r', escape: '\x1b', up: '\x1b[A', down: '\x1b[B', right: '\x1b[C', left: '\x1b[D' };
export const SECRET = `ghp_${'a1B2c3D4e5'.repeat(4)}`;

let tmp: string;
let savedDb: string | undefined;

/** The temporary root of the current test: its `home/octocode.db` is the shared agent database. */
export const tmpRoot = (): string => tmp;

/** Gives each test of the calling suite a fresh temporary root and agent database, restored afterwards. */
export function useBacklogHome(): void {
  beforeEach(() => {
    tmp = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'octo-backlog-')));
    savedDb = process.env.OCTOCODE_AGENT_DB;
    process.env.OCTOCODE_AGENT_DB = path.join(tmp, 'home', 'octocode.db');
  });

  afterEach(() => {
    closeSharedAgentDb();
    if (savedDb === undefined) delete process.env.OCTOCODE_AGENT_DB;
    else process.env.OCTOCODE_AGENT_DB = savedDb;
    fs.rmSync(tmp, { recursive: true, force: true });
  });
}

export function storeFor(repo: string, db: AgentDb = sharedAgentDb()): BacklogStore {
  return new BacklogStore(() => db, repo);
}

/** Record a session row, as the sessions feature does at session_start. */
export function session(id: string, pid: number | null, db: AgentDb = sharedAgentDb()): void {
  db.db.prepare(`INSERT INTO sessions (id, pid) VALUES (?, ?)`).run(id, pid);
}

export function repoDir(name = 'repo'): string {
  const dir = path.join(tmp, name);
  fs.mkdirSync(dir, { recursive: true });
  return dir;
}

export function setup(options: { isSubagent?: boolean; agentId?: string; parentSession?: string } = {}) {
  const fake = fakePi();
  const commands = new Subcommands();
  registerBacklog(fake.pi, { commands, isSubagent: options.isSubagent ?? false, ...(options.agentId ? { agentId: () => options.agentId } : {}), ...(options.parentSession ? { env: { ...process.env, [PARENT_SESSION_ENV]: options.parentSession } } : {}) });
  const cwd = repoDir();
  const tool = fake.tools.get('backlog');
  const call = async (params: Record<string, unknown>, ctx = fakeCtx({ cwd })): Promise<AgentToolResult<unknown>> => tool.execute('id', params, undefined, undefined, ctx);
  const text = async (params: Record<string, unknown>) => textOf((await call(params)).content[0]);
  return { ...fake, commands, cwd, tool, call, text, command: commands.get('backlog')! };
}
