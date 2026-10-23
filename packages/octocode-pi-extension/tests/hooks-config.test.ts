import fs from 'node:fs';
import path from 'node:path';
import { describe, expect, it } from 'vitest';
import { commandsFor, hookCalls, hookFiles, loadHooks, matches, parseHooks, toolAliases } from '../src/hooks/config.js';
import { tmp } from './helpers.js';
import { command, project, useHooksHome } from './hooks-harness.js';

useHooksHome();

describe('hooks config', () => {
  it('reads project hook files only in trusted projects', () => {
    const cwd = project({ PreToolUse: command('true') });
    const home = tmp();
    // ~/.claude/settings.json, ~/.codex/hooks.json, ~/.octocode/hooks.json
    expect(hookFiles(path.join(cwd), false, home)).toHaveLength(3);
    expect(loadHooks(cwd, false, home).config.PreToolUse).toHaveLength(0);
    expect(loadHooks(cwd, true, home).config.PreToolUse).toHaveLength(1);
  });

  it('keeps command hooks for mapped events and reports invalid JSON', () => {
    const config = parseHooks({ hooks: { PreToolUse: [{ matcher: 'Bash', hooks: [{ type: 'command', command: 'x', timeout: 5 }, { type: 'prompt', prompt: 'p' }] }], Stop: command('y') } }, 'f');
    expect(config.PreToolUse).toEqual([{ matcher: 'Bash', hooks: [{ command: 'x', timeoutMs: 5000, source: 'f' }] }]);
    const home = tmp();
    fs.mkdirSync(path.join(home, '.claude'));
    fs.writeFileSync(path.join(home, '.claude', 'settings.json'), '{oops');
    expect(loadHooks(tmp(), false, home).errors[0]).toContain('settings.json');
  });

  it('skips a missing hook file but reports one that exists and cannot be read', () => {
    const home = tmp();
    fs.mkdirSync(path.join(home, '.claude'));
    // A directory where the file belongs: reading it fails with EISDIR, not ENOENT.
    fs.mkdirSync(path.join(home, '.claude', 'settings.json'));
    const { errors } = loadHooks(tmp(), false, home);
    expect(errors).toEqual([expect.stringContaining(path.join(home, '.claude', 'settings.json'))]);
    expect(errors[0]).toMatch(/EISDIR/);
    expect(loadHooks(tmp(), false, tmp()).errors).toEqual([]);
  });

  it('reports a matcher that is not a valid pattern when the file loads, and the group matches nothing', () => {
    const home = tmp();
    fs.mkdirSync(path.join(home, '.claude'));
    fs.writeFileSync(path.join(home, '.claude', 'settings.json'), JSON.stringify({ hooks: { PreToolUse: command('x', 'Bash(') } }));
    const { config, errors } = loadHooks(tmp(), false, home);
    expect(errors).toEqual([`${path.join(home, '.claude', 'settings.json')}: PreToolUse matcher "Bash(" is not a valid pattern, so its hooks never run`]);
    expect(commandsFor(config, 'PreToolUse', toolAliases('bash'))).toEqual([]);
  });

  it('matches Claude tool names against Pi tools', () => {
    expect(matches('Bash', toolAliases('bash'))).toBe(true);
    expect(matches('Edit|Write', toolAliases('file'))).toBe(true);
    expect(matches('', ['anything'])).toBe(true);
    expect(matches('*', ['anything'])).toBe(true);
    expect(matches('Read', toolAliases('bash'))).toBe(false);
    expect(matches('(', ['x'])).toBe(false);
    expect(commandsFor(parseHooks({ hooks: { PreToolUse: command('a', 'Bash') } }, 'f'), 'PreToolUse', toolAliases('read'))).toEqual([]);
  });
});

describe('hook payloads', () => {
  it('sends each file query in Claude Code shape and maps read path to file_path', () => {
    const cwd = tmp();
    const abs = (name: string) => path.join(cwd, name);
    const calls = hookCalls(
      'file',
      {
        queries: [
          { reasoning: 'r', type: 'edit', path: 'a.ts', edits: [{ oldText: 'x', newText: 'y' }] },
          { reasoning: 'r', type: 'edit', path: 'b.ts', edits: [{ oldText: '1', newText: '2' }, { oldText: '3', newText: '4' }] },
          { reasoning: 'r', type: 'write', path: 'c.ts', content: 'body' },
          { reasoning: 'r', type: 'delete', path: 'd.ts' },
        ],
      },
      cwd,
    );
    expect(calls).toEqual([
      { names: ['file', 'Edit'], tool_name: 'Edit', tool_input: { file_path: abs('a.ts'), old_string: 'x', new_string: 'y' } },
      { names: ['file', 'MultiEdit'], tool_name: 'MultiEdit', tool_input: { file_path: abs('b.ts'), edits: [{ old_string: '1', new_string: '2' }, { old_string: '3', new_string: '4' }] } },
      { names: ['file', 'Write'], tool_name: 'Write', tool_input: { file_path: abs('c.ts'), content: 'body' } },
      { names: ['file', 'Delete'], tool_name: 'Delete', tool_input: { file_path: abs('d.ts') } },
    ]);
    expect(hookCalls('read', { path: 'a.ts', offset: 3 }, cwd)).toEqual([{ names: ['read', 'Read'], tool_name: 'Read', tool_input: { file_path: abs('a.ts'), offset: 3 } }]);
    expect(hookCalls('bash', { command: 'ls' }, cwd)).toEqual([{ names: ['bash', 'Bash'], tool_name: 'Bash', tool_input: { command: 'ls' } }]);
    expect(hookCalls('web', { url: 'u' }, cwd)[0]).toMatchObject({ tool_name: 'web', tool_input: { url: 'u' } });
    expect(hookCalls('file', { queries: 'nope' }, cwd)).toEqual([]);
  });
});
