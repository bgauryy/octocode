import { describe, expect, it, vi } from 'vitest';
import { say, Subcommands, wordCompletions } from '../src/shared/commands.js';
import { fakeCtx, fakePi } from './fake-pi.js';

const setup = () => {
  const commands = new Subcommands();
  const calls: string[] = [];
  commands.add('review', { description: 'review on|off — ask first', complete: (prefix) => wordCompletions([['on', 'ask'], 'off'], prefix), handler: async (args) => void calls.push(`review:${args}`) });
  commands.add('mcp', { description: 'mcp — servers', handler: async (args) => void calls.push(`mcp:${args}`) });
  const fake = fakePi();
  commands.register(fake.pi, () => 'STATUS');
  return { commands, calls, command: fake.commands.get('octocode') };
};

describe('/octocode subcommands', () => {
  it('routes status, help, subcommands and unknown names', async () => {
    const { calls, command } = setup();
    expect(command.description).toContain('mcp, review, help');
    const ctx = fakeCtx({ cwd: '.' });
    await command.handler('', ctx);
    await command.handler('status', ctx);
    await command.handler('help', ctx);
    await command.handler('  mcp   login  github ', ctx);
    await command.handler('review', ctx);
    await command.handler('nope', ctx);
    expect(calls).toEqual(['mcp:login  github', 'review:']);
    expect(ctx.ui.notes.map((note) => note.message.split('\n')[0])).toEqual(['STATUS', 'STATUS', 'Usage: /octocode [subcommand]', 'Unknown subcommand "nope".']);
    expect(ctx.ui.notes[2].message).toContain('review on|off — ask first');
    expect(ctx.ui.notes[2].message).toContain('(none) or status');
  });

  it('completes names, then arguments', () => {
    const { command } = setup();
    expect(command.getArgumentCompletions('re').map((item: { value: string }) => item.value)).toEqual(['review ']);
    expect(command.getArgumentCompletions('').map((item: { value: string }) => item.value)).toEqual(['mcp ', 'review ', 'help', 'status']);
    expect(command.getArgumentCompletions('sta')).toEqual([{ value: 'status', label: 'status', description: 'status — Octocode status' }]);
    expect(command.getArgumentCompletions('review o')).toEqual([
      { value: 'review on', label: 'on', description: 'ask' },
      { value: 'review off', label: 'off' },
    ]);
    expect(command.getArgumentCompletions('mcp x')).toBeNull();
    expect(command.getArgumentCompletions('zz')).toBeNull();
  });

  it('completes the same whitespace-separated commands that dispatch accepts', async () => {
    const { command, calls } = setup();
    expect(command.getArgumentCompletions(' \tre')).toEqual(command.getArgumentCompletions('re'));
    expect(command.getArgumentCompletions(' \treview\t  o')).toEqual(command.getArgumentCompletions('review o'));
    await command.handler(' \treview\t  on ', fakeCtx({ cwd: '.' }));
    expect(calls).toEqual(['review:on']);
  });

  it('shows registered shortcuts in help and keeps their completion and dispatch aligned', async () => {
    const { commands, calls } = setup();
    const fake = fakePi();
    commands.alias(fake.pi, 'review');
    commands.alias(fake.pi, 'mcp');
    expect(commands.help()).toContain('Shortcuts: /mcp, /review');
    const shortcut = fake.commands.get('review');
    expect(shortcut.getArgumentCompletions(' \to')).toEqual([{ value: 'on', label: 'on', description: 'ask' }, { value: 'off', label: 'off' }]);
    await shortcut.handler(' \ton ', fakeCtx({ cwd: '.' }));
    expect(calls).toEqual(['review:on']);
  });
});

describe('command output', () => {
  it('sanitizes notifications and writes headless output to stderr without touching stdout', () => {
    const stderr = vi.spyOn(process.stderr, 'write').mockReturnValue(true);
    const stdout = vi.spyOn(process.stdout, 'write').mockReturnValue(true);
    try {
      const interactive = fakeCtx({ cwd: '.', hasUI: true });
      const headless = fakeCtx({ cwd: '.', hasUI: false });
      const text = 'safe\u001b]0;title\u0007\u202e result';
      say(interactive, text, 'warning');
      expect(interactive.ui.notes.at(-1)).toEqual({ message: 'safe result', type: 'warning' });
      expect(stderr).not.toHaveBeenCalled();
      say(headless, text);
      expect(stderr).toHaveBeenCalledExactlyOnceWith('safe result\n');
      expect(stdout).not.toHaveBeenCalled();
    } finally {
      stderr.mockRestore();
      stdout.mockRestore();
    }
  });
});
