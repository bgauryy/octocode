import fs from 'node:fs';
import { describe, expect, it } from 'vitest';
import { Permission } from '../src/permission/ask.js';
import { ALLOW_ONCE, DENY, DENY_NOTE, NEEDS_APPROVAL } from '../src/permission/dialog.js';
import { AUDIT_NOTIFY_PREFIX, childAuditEntry, childAuditMessage } from '../src/permission/audit.js';
import { HEADER_PREFIX, permissionHeader, redactNonce, splitHeader, type PermissionHeader } from '../src/permission/header.js';
import type { PermissionRequest } from '../src/permission/request.js';
import { auditLines, foldLines } from '../src/permission/effect.js';
import { ownNonce, PERMISSION_ENV, PERMISSION_NONCE_FILE_ENV, PERMISSION_RELAY_ENV } from '../src/shared/env.js';
import { fakeCtx, fakePi } from './fake-pi.js';
import { tmp } from './helpers.js';
import { nonceFile } from './nonce-file.js';

const shell: PermissionRequest = { action: 'shell', title: 'Run this command?', what: ['npm test'], grant: { key: 'npm test', label: '`npm test`' } };

/** A subagent session, as Pi's RPC mode reports it (hasUI true), with the dialog titles it was asked. */
const NONCE = 'n0nce';
function child(env: NodeJS.ProcessEnv) {
  const fake = fakePi();
  const environment = { [PERMISSION_ENV]: 'manual', [PERMISSION_NONCE_FILE_ENV]: nonceFile(NONCE), ...env };
  const permission = new Permission({ pi: fake.pi, env: environment, isSubagent: true, stderr: () => undefined });
  const ctx = { ...fakeCtx({ cwd: tmp(), hasUI: true, mode: 'rpc', ui: { selects: [ALLOW_ONCE] } }), isProjectTrusted: () => true };
  const titles: string[] = [];
  const select = ctx.ui.select;
  ctx.ui.select = (async (title: string, ...rest: [string[], never]) => (titles.push(title), select(title, ...rest))) as never;
  permission.start(ctx);
  return { permission, ctx, titles, fake, environment };
}

describe('a subagent session that relays its dialogs', () => {
  it('asks through its own select, led by the header its parent reads', async () => {
    const { permission, ctx, titles } = child({ [PERMISSION_RELAY_ENV]: '1' });
    expect(await permission.check(ctx, { ...shell, also: ['net.private'], reasons: ['hook.ask'] })).toEqual({ allowed: true, via: 'user', lasts: 'call' });
    const { header, title } = splitHeader(titles[0]!, NONCE);
    expect(titles[0]!.split('\n')[0]!.startsWith(HEADER_PREFIX)).toBe(true);
    expect(header).toMatchObject({ action: 'shell', also: ['net.private'], reasons: ['hook.ask'], what: ['npm test'] });
    expect(header!.options).toEqual(expect.arrayContaining([ALLOW_ONCE, DENY, DENY_NOTE]));
    expect(title.startsWith('Run this command?')).toBe(true);
  });

  it('reads the nonce from its file once and unlinks it, and its environment never holds the secret', () => {
    const file = nonceFile(NONCE);
    const environment: NodeJS.ProcessEnv = { [PERMISSION_ENV]: 'manual', [PERMISSION_RELAY_ENV]: '1', [PERMISSION_NONCE_FILE_ENV]: file };
    expect(fs.readFileSync(file, 'utf8')).toBe(NONCE);
    new Permission({ pi: fakePi().pi, env: environment, isSubagent: true, stderr: () => undefined });
    expect(fs.existsSync(file)).toBe(false);
    expect(environment).not.toHaveProperty(PERMISSION_NONCE_FILE_ENV);
    expect(Object.values(environment)).not.toContain(NONCE);
    expect(ownNonce(environment)).toBe(NONCE);
  });

  it('keeps the nonce across a /reload in the same process, after the env was cleared', async () => {
    const saved = { ...process.env };
    try {
      process.env[PERMISSION_ENV] = 'manual';
      process.env[PERMISSION_RELAY_ENV] = '1';
      const file = nonceFile('reload-nonce');
      process.env[PERMISSION_NONCE_FILE_ENV] = file;
      const first = new Permission({ pi: fakePi().pi, isSubagent: true, stderr: () => undefined });
      expect(process.env[PERMISSION_NONCE_FILE_ENV]).toBeUndefined();
      expect(fs.existsSync(file)).toBe(false);
      void first;
      const fake = fakePi();
      const reloaded = new Permission({ pi: fake.pi, isSubagent: true, stderr: () => undefined });
      const ctx = { ...fakeCtx({ cwd: tmp(), hasUI: true, mode: 'rpc', ui: { selects: [ALLOW_ONCE] } }), isProjectTrusted: () => true };
      const titles: string[] = [];
      const select = ctx.ui.select;
      ctx.ui.select = (async (title: string, ...rest: [string[], never]) => (titles.push(title), select(title, ...rest))) as never;
      reloaded.start(ctx);
      await reloaded.check(ctx, shell);
      expect(splitHeader(titles[0]!, 'reload-nonce').header).toMatchObject({ action: 'shell' });
    } finally {
      delete (globalThis as Record<symbol, unknown>)[Symbol.for('octocode.permissionNonce')];
      process.env = saved;
    }
  });

  it('sends no header and no audit entry without a nonce: the parent would refuse both', async () => {
    const { permission, ctx, titles } = child({ [PERMISSION_RELAY_ENV]: '1', [PERMISSION_NONCE_FILE_ENV]: undefined });
    await permission.check(ctx, shell);
    permission.typed('ls', '/tmp');
    expect(titles[0]!.startsWith(HEADER_PREFIX)).toBe(false);
    expect(ctx.ui.notes.filter((note) => note.message.startsWith(AUDIT_NOTIFY_PREFIX))).toEqual([]);
  });

  it('says once on stderr that its audit is not forwarded when it has no nonce, and still records it in its own log', () => {
    const fake = fakePi();
    const written: string[] = [];
    const permission = new Permission({ pi: fake.pi, env: { [PERMISSION_ENV]: 'manual' }, isSubagent: true, stderr: (text) => void written.push(text) });
    const ctx = { ...fakeCtx({ cwd: tmp(), hasUI: true, mode: 'rpc' }), isProjectTrusted: () => true };
    permission.start(ctx);
    permission.typed('ls', '/tmp');
    permission.typed('pwd', '/tmp');
    expect(written.filter((text) => text.includes('no permission secret from its parent'))).toHaveLength(1);
    expect(fake.entries.filter((entry) => (entry.data as { via: string }).via === 'typed')).toHaveLength(2);
    expect(ctx.ui.notes.filter((note) => note.message.startsWith(AUDIT_NOTIFY_PREFIX))).toEqual([]);
  });

  it('keeps the old headless refusal without the relay variable', async () => {
    const { permission, ctx, titles } = child({});
    const verdict = await permission.check(ctx, shell);
    expect(verdict).toMatchObject({ allowed: false, via: 'headless' });
    expect(verdict.allowed ? '' : verdict.message).toContain(NEEDS_APPROVAL);
    expect(titles).toEqual([]);
  });

  it('sends every audit entry to its parent as a notify, whatever the relay setting', async () => {
    for (const env of [{ [PERMISSION_RELAY_ENV]: '1' }, {}]) {
      const { permission, ctx } = child(env);
      permission.typed('ls', '/tmp');
      const sent = ctx.ui.notes.map((note) => note.message).filter((message) => message.startsWith(AUDIT_NOTIFY_PREFIX));
      expect(sent).toHaveLength(1);
      expect(childAuditEntry(sent[0]!, 'child', NONCE)).toMatchObject({ action: 'shell', agent: 'child', via: 'typed', allowed: true });
    }
  });
});

describe('redactNonce', () => {
  it('replaces every occurrence of the nonce and leaves the rest whole', () => {
    expect(redactNonce(`a ${NONCE} b ${NONCE}`, NONCE)).toBe('a [nonce redacted] b [nonce redacted]');
    expect(redactNonce('nothing here', NONCE)).toBe('nothing here');
    expect(redactNonce('nothing here', '')).toBe('nothing here');
  });

  it('is what a rejected header shows: the title whole, the secret gone', () => {
    const line = permissionHeader({ action: 'shell', also: [], reasons: [], options: [ALLOW_ONCE], what: ['y'], nonce: NONCE });
    const title = `${line}\nRun it?\n  x`;
    const { header } = splitHeader(title, NONCE);
    expect(header).toBeUndefined();
    const shown = redactNonce(title, NONCE);
    expect(shown).not.toContain(NONCE);
    expect(shown).toContain('Run it?');
    expect(shown.startsWith(HEADER_PREFIX)).toBe(true);
  });
});

describe('splitHeader', () => {
  const header: PermissionHeader = { action: 'shell', also: [], reasons: [], options: [ALLOW_ONCE], what: ['x'], nonce: NONCE };
  const body = 'Run it?\n  x';
  it('returns the title as is without a header line, and parses only the first line', () => {
    expect(splitHeader('Run it?', NONCE)).toEqual({ title: 'Run it?' });
    const later = `${body}\n${permissionHeader(header)}`;
    expect(splitHeader(later, NONCE)).toEqual({ title: later });
    expect(splitHeader(`${permissionHeader(header)}\n${body}`, NONCE)).toEqual({ title: body, header });
  });

  it('returns the whole title, yielding no header, for a header line it cannot trust', () => {
    const lines = [
      `${HEADER_PREFIX}{`, `${HEADER_PREFIX}[]`,
      permissionHeader({ ...header, action: 'x' as never }), permissionHeader({ ...header, reasons: ['x'] as never }), `${HEADER_PREFIX}${JSON.stringify({ ...header, what: [1] })}`,
      permissionHeader({ ...header, nonce: 'other' }), permissionHeader({ ...header, nonce: '' }), `${HEADER_PREFIX}${JSON.stringify({ ...header, nonce: undefined })}`,
      // The targets it states are not the ones the dialog shows.
      permissionHeader({ ...header, what: ['rm -rf /'] }),
    ];
    for (const line of lines) {
      const title = `${line}\n${body}`;
      expect(splitHeader(title, NONCE), line).toEqual({ title });
    }
    expect(splitHeader(HEADER_PREFIX, NONCE)).toEqual({ title: HEADER_PREFIX });
    // No nonce to compare with: nothing is trusted, even a header that carries an empty one.
    expect(splitHeader(`${permissionHeader({ ...header, nonce: '' })}\n${body}`, '')).toEqual({ title: `${permissionHeader({ ...header, nonce: '' })}\n${body}` });
  });

  it('accepts the audit form of a target (redacted, one line) and a trailing overflow line', () => {
    const long = ['a'.repeat(300), 'two\nlines', 'c', 'd', 'e', 'f', 'g'];
    const what = [auditLines(long)].flat();
    const title = `${permissionHeader({ ...header, what })}\nRun it?\n${long.map((line) => `  ${foldLines(line)}`).join('\n')}`;
    expect(splitHeader(title, NONCE).header?.what).toEqual(what);
  });
});

describe('childAuditEntry', () => {
  const entry = { at: 5, mode: 'full', action: 'web', what: ['https://a.example', 'b', 'c', 'd', 'e', '+3 more'], allowed: true, via: 'mode', note: 'n', snapshot: 'refs/x', reasons: ['provenance'], suppressed: { web: 2 } };
  const message = (value: unknown, nonce: string | null = NONCE) => `${AUDIT_NOTIFY_PREFIX}${JSON.stringify({ ...(value as object), ...(nonce === null ? {} : { nonce }) })}`;

  it('keeps the validated fields, names the child from the process and keeps the rollup shape', () => {
    expect(childAuditEntry(message({ ...entry, agent: 'main' }), 'worker', NONCE)).toEqual({ ...entry, agent: 'worker' });
    expect(childAuditEntry(message({ ...entry, at: 'x' }), 'worker', NONCE)!.at).toBeTypeOf('number');
    // What the child's own module sends is accepted.
    const sent = childAuditMessage({ at: 1, mode: 'manual', action: 'shell', what: ['ls'], agent: 'child', allowed: true, via: 'typed' }, NONCE);
    expect(childAuditEntry(sent, 'worker', NONCE)).toMatchObject({ action: 'shell', via: 'typed', agent: 'worker' });
  });

  it('refuses what is not an entry, carries no nonce, or holds a value it does not know', () => {
    expect(childAuditEntry('hello', 'w', NONCE)).toBeUndefined();
    expect(childAuditEntry(`${AUDIT_NOTIFY_PREFIX}{`, 'w', NONCE)).toBeUndefined();
    expect(childAuditEntry(message(entry, null), 'w', NONCE)).toBeUndefined();
    expect(childAuditEntry(message(entry, 'other'), 'w', NONCE)).toBeUndefined();
    expect(childAuditEntry(message(entry), 'w', '')).toBeUndefined();
    const bads = [
      [], 3, { ...entry, action: 'nope' }, { ...entry, mode: 'x' }, { ...entry, allowed: 'yes' }, { ...entry, via: 1 }, { ...entry, via: 'because' }, { ...entry, what: 'x' }, { ...entry, what: [1] }, { ...entry, what: ['1', '2', '3', '4', '5', '6', '7'] },
      { ...entry, action: undefined }, { ...entry, reasons: ['provenance', 'bogus'] }, { ...entry, reasons: 'provenance' }, { ...entry, suppressed: { web: 'x' } }, { ...entry, suppressed: { bad: 1 } }, { ...entry, suppressed: { web: 1.5 } }, { ...entry, purpose: 7 }, { ...entry, note: {} },
    ];
    for (const bad of bads) expect(childAuditEntry(message(bad), 'w', NONCE), JSON.stringify(bad)).toBeUndefined();
  });
});
