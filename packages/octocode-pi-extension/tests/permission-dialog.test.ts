import { describe, expect, it } from 'vitest';
import { ALLOW_ONCE, DENY, DENY_NOTE, kindOption, NOTE_TITLE, selectDialog, sessionOption } from '../src/permission/dialog.js';
import type { PermissionRequest } from '../src/permission/request.js';
import { fakeCtx } from './fake-pi.js';

/** A dialog context that answers the select with `pick` (given the offered choices) and the note input with `note`. */
function asker(pick: (choices: string[]) => string | undefined, note?: string) {
  const seen: { title?: string; choices?: string[]; inputTitle?: string } = {};
  const ctx = fakeCtx({ cwd: '/' });
  ctx.ui.select = async (title: string, choices: string[]) => {
    Object.assign(seen, { title, choices });
    return pick(choices);
  };
  ctx.ui.input = async (title: string) => {
    seen.inputTitle = title;
    return note;
  };
  return { ctx, seen };
}

const request: PermissionRequest = { action: 'shell', title: 'Run this command?', what: ['rm -rf build'], grant: { key: 'rm', label: '`rm`' } };
const shellKind = kindOption('shell')!;
/** The answer the default dialog gives for `request` when the user picks `choice`. */
const answer = async (choice: string | undefined, forced = false) => selectDialog(asker(() => choice).ctx, request, { forced, mode: 'workspace' });

describe('selectDialog', () => {
  it('offers once, this call and its kind for the session, deny and deny with a note, in that order', async () => {
    const { ctx, seen } = asker(() => undefined);
    expect(await selectDialog(ctx, request, { forced: false, mode: 'workspace' })).toBeUndefined();
    expect(seen.choices).toEqual([ALLOW_ONCE, sessionOption('`rm`'), shellKind, DENY, DENY_NOTE]);
    expect(seen.title).toMatch(/^Run this command\?\n {2}rm -rf build/);
  });

  it('offers nothing session-wide when forced, and no kind for a call of several classes', async () => {
    const forced = asker(() => undefined);
    await selectDialog(forced.ctx, request, { forced: true, mode: 'workspace' });
    expect(forced.seen.choices).toEqual([ALLOW_ONCE, DENY, DENY_NOTE]);
    const several = asker(() => undefined);
    await selectDialog(several.ctx, { ...request, also: ['net.private'] }, { forced: false, mode: 'workspace' });
    expect(several.seen.choices).toEqual([ALLOW_ONCE, sessionOption('`rm`'), DENY, DENY_NOTE]);
  });

  it('maps each offered choice to its answer; an unoffered reply decides nothing', async () => {
    expect(await answer(ALLOW_ONCE)).toBe('once');
    expect(await answer(sessionOption('`rm`'))).toBe('session');
    expect(await answer(shellKind)).toBe('kind');
    expect(await answer(DENY)).toBe('deny');
    expect(await answer('something else')).toBeUndefined();
    // A session choice that was not offered (forced) decides nothing.
    expect(await answer(sessionOption('`rm`'), true)).toBeUndefined();
    expect(await answer(shellKind, true)).toBeUndefined();
  });

  it('asks why after deny with a note: the trimmed note, a plain deny when empty, dismissed when cancelled', async () => {
    const withNote = asker(() => DENY_NOTE, '  use make clean  ');
    expect(await selectDialog(withNote.ctx, request, { forced: false, mode: 'workspace' })).toEqual({ deny: 'use make clean' });
    expect(withNote.seen.inputTitle).toBe(NOTE_TITLE);
    expect(await selectDialog(asker(() => DENY_NOTE, '   ').ctx, request, { forced: false, mode: 'workspace' })).toBe('deny');
    expect(await selectDialog(asker(() => DENY_NOTE).ctx, request, { forced: false, mode: 'workspace' })).toBeUndefined();
  });

  it('leads a relayed child dialog with the header its parent reads, only when the call has a class', async () => {
    const relayed = asker(() => undefined);
    await selectDialog(relayed.ctx, request, { forced: false, mode: 'workspace', relayNonce: 'nonce-1' });
    expect(relayed.seen.title).toContain('nonce-1');
    expect(relayed.seen.title?.split('\n')[1]).toBe('Run this command?');
    const classless = asker(() => undefined);
    const { action: _action, ...noAction } = request;
    await selectDialog(classless.ctx, noAction, { forced: false, mode: 'workspace', relayNonce: 'nonce-1' });
    expect(classless.seen.title).toMatch(/^Run this command\?/);
  });
});
