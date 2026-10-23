import { describe, expect, it } from 'vitest';
import { ACTION_CLASSES, actionWhy, ASK_REASONS, declaredPermissions, decide, kindOf, modeSummary, parseGrants, reasonAsks, SPAWN_GRANTABLE, startMode, UNGRANTABLE, type ActionClass, type AskReason, type PermissionMode } from '../src/permission/mode.js';
import { PERMISSION_ENV, PERMISSION_GRANTS_ENV } from '../src/shared/env.js';

/** Plan §2.4, one row per action class: manual, workspace, full. */
const MATRIX: Record<ActionClass, [string, string, string]> = {
  'read.outside': ['ask', 'allow', 'allow'],
  'file.workspace': ['ask', 'allow', 'allow'],
  'file.outside': ['ask', 'ask', 'allow'],
  shell: ['ask', 'allow', 'allow'],
  'shell.destructive': ['ask', 'ask', 'allow'],
  web: ['ask', 'allow', 'allow'],
  browser: ['ask', 'allow', 'allow'],
  'browser.control': ['ask', 'ask', 'allow'],
  'net.private': ['ask', 'ask', 'allow'],
  'upload.outside': ['ask', 'ask', 'allow'],
  'mcp.write': ['ask', 'allow', 'allow'],
  'mcp.destructive': ['ask', 'ask', 'allow'],
  'agent.spawn': ['ask', 'allow', 'allow'],
};

/** Reasons tighten an action. They are not action classes: manual, workspace, full. */
const REASONS: Record<AskReason, [string, string, string]> = {
  'hook.ask': ['ask', 'ask', 'allow'],
  provenance: ['ask', 'ask', 'allow'],
  'profile.permissions': ['ask', 'ask', 'allow'],
};

describe('permission modes', () => {
  it('decides every action class in every mode as the plan table says', () => {
    expect([...ACTION_CLASSES].sort()).toEqual(Object.keys(MATRIX).sort());
    expect(ACTION_CLASSES).not.toContain('provenance');
    expect(ACTION_CLASSES).not.toContain('hook.ask');
    for (const [action, row] of Object.entries(MATRIX) as Array<[ActionClass, string[]]>) {
      (['manual', 'workspace', 'full'] as PermissionMode[]).forEach((mode, at) => expect(decide(mode, action), `${action} in ${mode}`).toBe(row[at]));
    }
    expect([...ASK_REASONS].sort()).toEqual(Object.keys(REASONS).sort());
    for (const [reason, row] of Object.entries(REASONS) as Array<[AskReason, string[]]>) {
      (['manual', 'workspace', 'full'] as PermissionMode[]).forEach((mode, at) => expect(reasonAsks(mode, reason) ? 'ask' : 'allow', `${reason} in ${mode}`).toBe(row[at]));
    }
  });

  it('starts every session in full unless the user chose a mode', () => {
    expect(startMode({})).toEqual({ mode: 'full' });
    expect(startMode({ [PERMISSION_ENV]: '' })).toEqual({ mode: 'full' });
  });

  it('honours an exported OCTOCODE_PERMISSION, full included (the user\'s own choice), failing toward manual', () => {
    expect(startMode({ [PERMISSION_ENV]: 'manual' })).toEqual({ mode: 'manual' });
    expect(startMode({ [PERMISSION_ENV]: ' Workspace ' })).toEqual({ mode: 'workspace' });
    expect(startMode({ [PERMISSION_ENV]: 'full' })).toEqual({ mode: 'full' });
    // An invalid value starts stricter, never looser, with one warning naming the value.
    expect(startMode({ [PERMISSION_ENV]: 'yolo' })).toMatchObject({ mode: 'manual', warning: expect.stringContaining('yolo') });
  });

  it('says what each mode does', () => {
    expect(modeSummary('full')).toMatch(/destructive commands.*private web fetches.*hook's ask.*run without a prompt/);
    expect(modeSummary('full')).toMatch(/⚠ counter.*audit log.*no UI has only the log/);
    expect(modeSummary('workspace')).toContain('shell.destructive');
    expect(modeSummary('workspace')).toContain('browser.control');
    expect(modeSummary('manual')).toContain('hook asks, provenance hits and profile permissions also ask');
  });

  it('gives every class a reason line and a noun, and never a session kind to the per-call classes', () => {
    for (const action of ACTION_CLASSES) expect(actionWhy(action), action).toMatch(/\S/);
    expect([...UNGRANTABLE].sort()).toEqual(['browser.control', 'mcp.destructive', 'shell.destructive']);
    for (const action of ['browser.control', 'shell.destructive', 'mcp.destructive'] as const) expect(kindOf(action)).toBeUndefined();
  });

  it('reads a profile\'s permissions: the declarable classes only, in any list shape', () => {
    expect(declaredPermissions(['browser.control', 'net.private', 'upload.outside'])).toEqual({ declared: ['browser.control', 'net.private', 'upload.outside'], ignored: [] });
    expect(declaredPermissions('browser.control, shell, browser.control')).toEqual({ declared: ['browser.control'], ignored: ['shell'] });
    expect(declaredPermissions('net.private')).toEqual({ declared: ['net.private'], ignored: [] });
    expect(declaredPermissions(['shell.destructive', 7])).toEqual({ declared: [], ignored: ['shell.destructive', '7'] });
    expect(declaredPermissions(undefined)).toEqual({ declared: [], ignored: [] });
  });

  it('carries every grantable kind in a child grant, never a tripwire', () => {
    expect([...parseGrants({ [PERMISSION_GRANTS_ENV]: 'file.workspace, shell' })].sort()).toEqual(['file.workspace', 'shell']);
    expect([...parseGrants({ [PERMISSION_GRANTS_ENV]: 'file.outside,net.private,provenance,hook.ask,shell,shell.destructive,browser.control,profile.permissions' })].sort()).toEqual(['browser.control', 'file.outside', 'net.private', 'shell']);
    expect(SPAWN_GRANTABLE.has('shell.destructive')).toBe(false);
    expect(parseGrants({}).size).toBe(0);
  });
});
