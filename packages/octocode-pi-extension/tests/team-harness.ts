import { vi } from 'vitest';
import { INTERNAL_MESSAGE_TYPE } from '../src/shared/messages.js';
import type { Member } from '../src/team/model.js';
import { Team } from '../src/team/session.js';
import { TeamStore } from '../src/team/store.js';
import { registerCollab } from '../src/team/tools.js';
import { fakeCtx, fakePi } from './fake-pi.js';

/** A member row of this process; `extra` overrides any field. */
export const member = (id: string, extra: Partial<Member> = {}): Member => ({
  id, role: id.replace(/-[0-9a-f]{6}$/, ''), pid: process.pid, status: 'idle', joinedAt: 1_000, updatedAt: Date.now(), toolCalls: 0, input: 0, cacheRead: 0, output: 0, cost: 0, unpriced: false, ...extra,
});

/** Read what is pending and acknowledge it, as a recipient does after injecting it. */
export const take = (store: TeamStore, id: string) => {
  const got = store.pending(id);
  store.ack(id, got.map((m) => m.id));
  return got;
};

export const sent = (result: { id: number | undefined }) => result.id!;

// Team sessions on a fake Pi and stores a test opened; `closeTeams` (after each test) stops and closes them.
const stores: TeamStore[] = [];
const teams: Array<{ team: Team; ctx: ReturnType<typeof fakeCtx> }> = [];

/** A store over `file` with no environment, closed after the test. */
export const openStore = (cwd: string, file: string) => {
  const store = TeamStore.open(cwd, file, {});
  stores.push(store);
  return store;
};

/** A started team session with the team tools registered on a fake Pi, stopped after the test. */
export function agent(dbFile: string, cwd: string, env: Record<string, string> = {}, { hasUI = true, idle = true }: { hasUI?: boolean; idle?: boolean } = {}) {
  const fake = fakePi();
  const team = new Team(fake.pi, { OCTOCODE_AGENT_DB: dbFile, ...env });
  registerCollab(fake.pi, team);
  const ctx = fakeCtx({ cwd, hasUI, idle });
  team.start(ctx);
  teams.push({ team, ctx });
  return { ...fake, team, ctx };
}

/** Tracks a team session a test started by hand, so it is stopped after the test. */
export const trackTeam = (team: Team, ctx: ReturnType<typeof fakeCtx>) => void teams.push({ team, ctx });

export function closeTeams(): void {
  vi.useRealTimers();
  for (const { team } of teams.splice(0)) team.stop();
  for (const store of stores.splice(0)) store.close();
}

/** Fake timers and clock for a test that waits for the ticker to do nothing: `advanceTimersByTimeAsync` runs its ticks without waiting in real time. */
export const fakeClock = () => vi.useFakeTimers({ toFake: ['setInterval', 'clearInterval', 'setTimeout', 'clearTimeout', 'Date'] });

/** What Pi was handed with `sendMessage` (the fake Pi's log). */
type Handed = ReturnType<typeof fakePi>['sent'];

/** The team messages Pi was handed, in order. */
export const teamMail = (handed: Handed) => handed.filter((entry) => entry.message.customType === INTERNAL_MESSAGE_TYPE && Array.isArray(entry.message.details?.messages));

/**
 * Pi is about to call the model with this message in the conversation: it emits `context` with the messages. (An FYI
 * appended while idle, or flushed at the end of a turn, reaches extensions no other way: Pi's append never emits
 * message_start to them.)
 */
export const seen = (who: ReturnType<typeof agent>, entry: Handed[number]) =>
  who.emit('context', { messages: [{ role: 'user', content: 'task' }, { role: 'custom', customType: INTERNAL_MESSAGE_TYPE, content: entry.message.content, details: entry.message.details }] }, who.ctx);

/** Team mail Pi was handed that carries numbered messages. */
export const mailOf = (handed: Handed) => handed.filter((entry) => entry.message.customType === INTERNAL_MESSAGE_TYPE && entry.message.details?.messages?.some((m: { id?: number }) => typeof m.id === 'number'));
