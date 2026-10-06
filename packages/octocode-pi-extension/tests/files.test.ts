import fs from 'node:fs';
import path from 'node:path';
import { describe, expect, it, vi } from 'vitest';
import { initTheme } from '@earendil-works/pi-coding-agent';
import { pathToFileURL } from 'node:url';
import { diffStats, querySize } from '../src/files/render.js';
import { FileGuard, canonicalPath, formatOutcomes, readPaths, registerFileTool, returnedContent } from '../src/files/tool.js';
import { Checkpoints } from '../src/files/checkpoint.js';
import { sha256 } from '../src/shared/atomic.js';
import { resolveToolPath } from '../src/shared/home.js';
import { fakeCtx } from './fake-pi.js';
import { bashSafetyGate, catastrophicCommand } from '../src/files/bash-guard.js';
import { tmp } from './helpers.js';
import { theme } from './fake-pi.js';

describe('file tool', () => {
  it('allows writing new or unread files, and requires existing files for edit/delete', () => {
    const dir = tmp();
    const file = path.join(dir, 'a.txt');
    const guard = new FileGuard();
    expect(guard.check(file, 'write', 'a.txt')).toBeUndefined();
    expect(guard.check(file, 'edit', 'a.txt')).toMatch(/does not exist/);
    fs.writeFileSync(file, 'one');
    expect(guard.check(file, 'write', 'a.txt')).toBeUndefined();
    expect(guard.check(file, 'edit', 'a.txt')).toBeUndefined();
  });

  it('detects files changed on disk after the last read and resets after compaction', () => {
    const dir = tmp();
    const file = path.join(dir, 'a.txt');
    fs.writeFileSync(file, 'one');
    const guard = new FileGuard();
    guard.record(file);
    expect(guard.check(file, 'edit', 'a.txt')).toBeUndefined();
    fs.writeFileSync(file, 'changed by someone else');
    expect(guard.check(file, 'edit', 'a.txt')).toMatch(/changed on disk/);
    guard.reset();
    // After compaction the old read is out of context: the file must be read again, even unchanged.
    expect(guard.check(file, 'edit', 'a.txt')).toMatch(/Read it again/);
    guard.record(file);
    expect(guard.check(file, 'edit', 'a.txt')).toBeUndefined();
  });

  it('makes a forgotten (rewound) file need a fresh read, leaves never-read and deleted files alone', () => {
    const dir = tmp();
    const file = path.join(dir, 'a.txt');
    const other = path.join(dir, 'b.txt');
    fs.writeFileSync(file, 'one');
    fs.writeFileSync(other, 'two');
    const guard = new FileGuard();
    guard.record(file);
    guard.forget(file);
    expect(guard.check(file, 'write', 'a.txt')).toMatch(/rewound or compacted[\s\S]*Read it again/);
    expect(guard.check(other, 'edit', 'b.txt')).toBeUndefined();
    guard.record(file);
    expect(guard.check(file, 'write', 'a.txt')).toBeUndefined();
    // A rewound file that is gone can be created again without a read.
    guard.forget(file);
    fs.rmSync(file);
    expect(guard.check(file, 'write', 'a.txt')).toBeUndefined();
  });

  it('records a read by stat without reading the file, and still catches a same-size rewrite that kept the mtime', () => {
    const dir = tmp();
    const file = path.join(dir, 'small.txt');
    fs.writeFileSync(file, 'aaaa');
    const pinned = new Date(1_700_000_000_000);
    fs.utimesSync(file, pinned, pinned);
    const guard = new FileGuard();
    const reads = vi.spyOn(fs, 'readFileSync');
    try {
      guard.record(file);
      expect(guard.inspect(file, 'edit', 'small.txt')).toEqual({ current: expect.objectContaining({ size: 4 }) });
      expect(reads).not.toHaveBeenCalled();
    } finally {
      reads.mockRestore();
    }
    fs.writeFileSync(file, 'bbbb');
    fs.utimesSync(file, pinned, pinned);
    expect(guard.check(file, 'edit', 'small.txt')).toMatch(/changed on disk/);
  });

  it('checks a known digest against the content, so a touch passes and a same-size rewrite does not', () => {
    const dir = tmp();
    const file = path.join(dir, 'a.txt');
    fs.writeFileSync(file, 'aaaa');
    const guard = new FileGuard();
    guard.record(file, { sha256: sha256('aaaa'), size: 4 });
    fs.utimesSync(file, new Date(), new Date(Date.now() + 5_000));
    expect(guard.inspect(file, 'edit', 'a.txt')).toMatchObject({ expected: sha256('aaaa') });
    fs.appendFileSync(file, 'x');
    expect(guard.check(file, 'edit', 'a.txt')).toMatch(/changed on disk/);
    // A digest of a different size (not the file's verbatim text) is not kept.
    guard.record(file, { sha256: sha256('other'), size: 99 });
    expect(guard.inspect(file, 'edit', 'a.txt').expected).toBeUndefined();
  });

  it('counts a symlink or case alias of a read file as read', () => {
    const dir = tmp();
    const file = path.join(dir, 'Real.txt');
    fs.writeFileSync(file, 'one');
    fs.symlinkSync(file, path.join(dir, 'link.txt'));
    const guard = new FileGuard();
    guard.record(path.join(dir, 'link.txt'));
    expect(guard.check(file, 'write', 'Real.txt')).toBeUndefined();
    const insensitive = fs.existsSync(path.join(dir, 'real.txt'));
    if (insensitive) expect(guard.check(path.join(dir, 'real.txt'), 'write', 'real.txt')).toBeUndefined();
    fs.writeFileSync(file, 'bbbb');
    expect(guard.check(file, 'write', 'Real.txt')).toMatch(/changed on disk/);
    guard.forget(path.join(dir, 'link.txt'));
    expect(guard.check(file, 'write', 'Real.txt')).toMatch(/Read it again/);
    // A missing file resolves through its real parent directory; an unresolvable one stays as given.
    expect(canonicalPath(path.join(dir, 'new.txt'))).toBe(path.join(fs.realpathSync.native(dir), 'new.txt'));
    expect(canonicalPath('/no/such/dir/x')).toBe('/no/such/dir/x');
  });

  it('takes the digest of what Pi read returned only when it is the whole file', () => {
    const text = [{ type: 'text', text: 'hello' }];
    expect(returnedContent('read', { path: 'a' }, text, undefined)).toEqual({ sha256: sha256('hello'), size: 5 });
    expect(returnedContent('read', { path: 'a', offset: 2 }, text, undefined)).toBeUndefined();
    expect(returnedContent('read', { path: 'a' }, text, { truncation: {} })).toBeUndefined();
    expect(returnedContent('read', { path: 'a' }, [...text, { type: 'image' }], undefined)).toBeUndefined();
    expect(returnedContent('octocode_localGetFileContent', { path: 'a' }, text, undefined)).toBeUndefined();
  });

  it('records a Pi read from the returned text: a change between the read and the hook is caught', async () => {
    const cwd = tmp();
    const file = path.join(cwd, 'a.txt');
    fs.writeFileSync(file, 'new!');
    const guard = new FileGuard();
    const handlers = new Map<string, (event: unknown, ctx: unknown) => Promise<void>>();
    registerFileTool({ registerTool: () => undefined, on: (name: string, handler: never) => handlers.set(name, handler) } as never, guard);
    // The model saw 'old!'; the disk already holds 'new!' when the hook runs.
    await handlers.get('tool_result')!({ toolName: 'read', input: { path: 'a.txt' }, content: [{ type: 'text', text: 'old!' }], isError: false }, { cwd });
    expect(guard.inspect(file, 'edit', 'a.txt').expected).toBe(sha256('old!'));
  });

  it('clears a re-read refusal after a whole-file Octocode MCP read, but not after a partial one', async () => {
    const cwd = tmp();
    const file = path.join(cwd, 'a.txt');
    fs.writeFileSync(file, 'one');
    const guard = new FileGuard();
    const handlers = new Map<string, (event: unknown, ctx: unknown) => Promise<void>>();
    registerFileTool({ registerTool: () => undefined, on: (name: string, handler: never) => handlers.set(name, handler) } as never, guard);
    const mcpRead = (query: Record<string, unknown>) => handlers.get('tool_result')!({ toolName: 'mcp__octocode__localGetFileContent', input: { queries: [{ path: file, ...query }] }, content: [{ type: 'text', text: 'one' }], isError: false }, { cwd });
    await mcpRead({ fullContent: true });
    expect(guard.inspect(file, 'edit', 'a.txt').refusal).toBeUndefined();
    // Its read result was trimmed: the content left the context.
    guard.forget(file);
    expect(guard.inspect(file, 'edit', 'a.txt').refusal).toMatch(/or its read result was trimmed/);
    await mcpRead({ startLine: 1, endLine: 1 });
    expect(guard.inspect(file, 'edit', 'a.txt').refusal).toMatch(/Read it again/);
    await mcpRead({ fullContent: true });
    expect(guard.inspect(file, 'edit', 'a.txt').refusal).toBeUndefined();
  });

  it('stops applying a batch once it is aborted: a later delete does not run', async () => {
    const cwd = tmp();
    fs.writeFileSync(path.join(cwd, 'b.txt'), 'keep');
    let tool: { execute: (...args: unknown[]) => Promise<unknown> } | undefined;
    registerFileTool({ registerTool: (t: typeof tool) => (tool = t), on: () => undefined } as never, new FileGuard());
    // Not aborted when the batch starts, aborted from the first query on (the user pressed Esc mid-batch).
    let checks = 0;
    const reason = new Error('aborted');
    const signal = { get aborted() { return ++checks > 1; }, reason, addEventListener: () => undefined, removeEventListener: () => undefined, throwIfAborted: () => { if (signal.aborted) throw reason; } } as unknown as AbortSignal;
    const batch = { queries: [{ reasoning: 'r', type: 'write', path: 'a.txt', content: 'x' }, { reasoning: 'r', type: 'delete', path: 'b.txt' }] };
    await expect(tool!.execute('c', batch, signal, undefined, { cwd, hasUI: false })).rejects.toThrow(/2\. FAILED delete b\.txt: Not applied: the batch was aborted/);
    expect(fs.readFileSync(path.join(cwd, 'b.txt'), 'utf8')).toBe('keep');
  });

  it('records the digest of what the file tool wrote, so the next change needs no re-read but a later rewrite is still caught', async () => {
    const cwd = tmp();
    const file = path.join(cwd, 'a.txt');
    fs.writeFileSync(file, 'one');
    const guard = new FileGuard();
    let tool: { execute: (...args: unknown[]) => Promise<unknown> } | undefined;
    registerFileTool({ registerTool: (t: typeof tool) => (tool = t), on: () => undefined } as never, guard);
    const ctx = { cwd, hasUI: false };
    await tool!.execute('c', { queries: [{ reasoning: 'r', type: 'write', path: 'b.txt', content: 'new' }] }, undefined, undefined, ctx);
    await tool!.execute('c', { queries: [{ reasoning: 'r', type: 'edit', path: 'a.txt', edits: [{ oldText: 'one', newText: 'two' }] }] }, undefined, undefined, ctx);
    expect(guard.check(file, 'write', 'a.txt')).toBeUndefined();
    expect(guard.check(path.join(cwd, 'b.txt'), 'write', 'b.txt')).toBeUndefined();
    const { atime, mtime } = fs.statSync(file);
    fs.writeFileSync(file, 'TWO');
    fs.utimesSync(file, atime, mtime);
    await expect(tool!.execute('c', { queries: [{ reasoning: 'r', type: 'edit', path: 'a.txt', edits: [{ oldText: 'TWO', newText: 'x' }] }] }, undefined, undefined, ctx)).rejects.toThrow(/changed on disk/);
    expect(fs.readFileSync(file, 'utf8')).toBe('TWO');
  });

  it('learns reads from Pi read and whole-file Octocode MCP reads only', () => {
    expect(readPaths('read', { path: '@src/x.ts' }, '/r')).toEqual(['/r/src/x.ts']);
    expect(readPaths('octocode_localGetFileContent', { queries: [{ path: '/r/a.ts', fullContent: true }] }, '/r')).toEqual([]);
    const queries = [
      { path: '/r/whole.ts', fullContent: true },
      { path: '/r/plain.ts', fullContent: true, minify: 'none' },
      { path: '/r/default-view.ts' },
      { path: '/r/range.ts', startLine: 1, endLine: 5 },
      { path: '/r/match.ts', matchString: 'x' },
      { path: '/r/window.ts', fullContent: true, charOffset: 0, charLength: 100 },
      { path: '/r/minified.ts', fullContent: true, minify: 'standard' },
    ];
    expect(readPaths('mcp__octocode__localGetFileContent', { queries }, '/r')).toEqual(['/r/whole.ts', '/r/plain.ts']);
    expect(readPaths('mcp__octocode__localGetFileContent', { queries: 'nope' }, '/r')).toEqual([]);
    expect(resolveToolPath('/repo', '@src/x.ts')).toBe('/repo/src/x.ts');
  });

  it('resolves paths as Pi tools do: ~, file:// URLs, Unicode spaces and a leading @', () => {
    const home = tmp();
    vi.stubEnv('HOME', home);
    try {
      expect(resolveToolPath('/r', '~')).toBe(home);
      expect(resolveToolPath('/r', '~/.bashrc')).toBe(path.join(home, '.bashrc'));
      expect(resolveToolPath('/r', '@~/x')).toBe(path.join(home, 'x'));
      expect(resolveToolPath('~/repo', 'a.ts')).toBe(path.join(home, 'repo', 'a.ts'));
      expect(resolveToolPath('/r', pathToFileURL('/tmp/a b.ts').href)).toBe('/tmp/a b.ts');
      expect(resolveToolPath('/r', 'my\u00a0file\u202f.txt')).toBe('/r/my file .txt');
      expect(resolveToolPath('/r', '~user/x')).toBe('/r/~user/x');
      expect(readPaths('read', { path: '~/notes.md' }, '/r')).toEqual([path.join(home, 'notes.md')]);
    } finally {
      vi.unstubAllEnvs();
    }
  });

  it('writes, re-reads and edits a ~ path at the file Pi touches, checkpointing it', async () => {
    const home = tmp();
    const cwd = tmp();
    vi.stubEnv('HOME', home);
    try {
      const file = path.join(home, '.bashrc');
      fs.writeFileSync(file, 'one');
      const guard = new FileGuard();
      const checkpoints = new Checkpoints();
      const store = tmp();
      checkpoints.open(store);
      const handlers = new Map<string, (event: unknown, ctx: unknown) => Promise<void>>();
      let tool: { execute: (...args: unknown[]) => Promise<unknown> } | undefined;
      registerFileTool({ registerTool: (t: typeof tool) => (tool = t), on: (name: string, handler: never) => handlers.set(name, handler) } as never, guard, undefined, checkpoints);
      const ctx = { cwd, hasUI: false };
      await handlers.get('tool_result')!({ toolName: 'read', input: { path: '~/.bashrc' }, content: [{ type: 'text', text: 'one' }], isError: false }, ctx);
      await tool!.execute('c', { queries: [{ reasoning: 'r', type: 'edit', path: '~/.bashrc', edits: [{ oldText: 'one', newText: 'two' }] }] }, undefined, undefined, ctx);
      await tool!.execute('c', { queries: [{ reasoning: 'r', type: 'write', path: '~/.bashrc', content: 'three' }] }, undefined, undefined, ctx);
      await tool!.execute('c', { queries: [{ reasoning: 'r', type: 'write', path: '~/new.txt', content: 'n' }] }, undefined, undefined, ctx);
      expect(fs.readFileSync(file, 'utf8')).toBe('three');
      expect(fs.readFileSync(path.join(home, 'new.txt'), 'utf8')).toBe('n');
      expect(fs.existsSync(path.join(cwd, '~'))).toBe(false);
      const journaled = fs.readFileSync(path.join(store, 'journal.jsonl'), 'utf8').split('\n').filter(Boolean).map((line) => (JSON.parse(line) as { path: string }).path);
      expect(new Set(journaled)).toEqual(new Set([file, path.join(home, 'new.txt')]));
    } finally {
      vi.unstubAllEnvs();
    }
  });

  it('keeps a change that succeeded when its checkpoint cannot be saved, and says so once', async () => {
    const cwd = tmp();
    const checkpoints = new Checkpoints();
    checkpoints.open(tmp());
    vi.spyOn(checkpoints, 'settle').mockRejectedValue(new Error('disk full'));
    let tool: { execute: (...args: unknown[]) => Promise<{ content: Array<{ text: string }> }> } | undefined;
    registerFileTool({ registerTool: (t: typeof tool) => (tool = t), on: () => undefined } as never, new FileGuard(), undefined, checkpoints);
    const ctx = fakeCtx({ cwd });
    const first = await tool!.execute('c', { queries: [{ reasoning: 'r', type: 'write', path: 'a.txt', content: 'a' }, { reasoning: 'r', type: 'write', path: 'b.txt', content: 'b' }] }, undefined, undefined, ctx);
    expect(first.content[0]!.text).toMatch(/1\. OK write a\.txt.*checkpoint not saved[\s\S]*2\. OK write b\.txt.*checkpoint not saved/);
    expect(fs.readFileSync(path.join(cwd, 'a.txt'), 'utf8')).toBe('a');
    expect(ctx.ui.notes.filter((note) => /could not save a checkpoint \(disk full\)/.test(note.message))).toHaveLength(1);
  });

  it('reports each query independently', () => {
    expect(
      formatOutcomes([
        { type: 'write', path: 'a', reasoning: 'r', ok: true, message: 'Wrote a' },
        { type: 'edit', path: 'b', reasoning: 'r', ok: false, message: 'no match' },
      ]),
    ).toBe('1. OK write a: Wrote a\n2. FAILED edit b: no match');
    expect(
      formatOutcomes([
        { type: 'edit', path: 'src/a.ts', reasoning: 'r', ok: true, message: 'Successfully replaced 2 block(s) in src/a.ts.' },
        { type: 'write', path: 'b(1).md', reasoning: 'r', ok: true, message: 'Successfully wrote 12 bytes to b(1).md' },
        { type: 'edit', path: 'c.ts', reasoning: 'r', ok: false, message: 'Could not find the exact text in c.ts.' },
      ]),
    ).toBe('1. OK edit src/a.ts: replaced 2 block(s)\n2. OK write b(1).md: wrote 12 bytes\n3. FAILED edit c.ts: Could not find the exact text in c.ts.');
  });
});

describe('file change rendering', () => {

  it('shows each file change with its reasoning, size, outcome and a capped diff', () => {
    initTheme('dark');
    let tool: { renderCall: Function; renderResult: Function } | undefined;
    registerFileTool({ registerTool: (definition: never) => (tool = definition), on: () => undefined } as never, new FileGuard());
    const args = {
      queries: [
        { reasoning: 'Fix the off-by-one in the loop', type: 'edit', path: 'src/a.ts', edits: [{ oldText: 'a', newText: 'b' }, { oldText: 'c', newText: 'd' }] },
        { reasoning: 'Add the missing test', type: 'write', path: 'test/a.test.ts', content: 'one\ntwo' },
      ],
    };
    const call = tool!.renderCall(args, theme, { lastComponent: undefined, isPartial: true, executionStarted: false }).render(120).join('\n');
    expect(call.split('\n')[0]).toBe('○ File(2 changes)');
    expect(call).toMatch(/edit src\/a\.ts · 2 edits/);
    expect(call).toMatch(/write test\/a\.test\.ts · 2 lines/);
    // Reasoning shows only when expanded.
    expect(call).not.toContain('↳');
    const expandedCall = tool!.renderCall(args, theme, { lastComponent: undefined, expanded: true }).render(120).join('\n');
    expect(expandedCall).toContain('↳ Fix the off-by-one in the loop');
    expect(expandedCall).toContain('↳ Add the missing test');
    const single = tool!.renderCall({ queries: [args.queries[0]] }, theme, { lastComponent: undefined, isPartial: false, isError: false, state: { durationMs: 1200 } }).render(120).join('\n');
    expect(single).toBe('● File(edit src/a.ts · 2 edits) · 1.2s');
    const diff = Array.from({ length: 30 }, (_, index) => `+${index} added line ${index}`).join('\n');
    const result = { content: [], details: { outcomes: [{ type: 'edit', path: 'src/a.ts', reasoning: '', ok: true, message: 'ok', diff }, { type: 'write', path: 'test/a.test.ts', reasoning: '', ok: false, message: 'has not been read' }] } };
    const collapsed = tool!.renderResult(result, { expanded: false, isPartial: false }, theme, { lastComponent: undefined, isError: false }).render(120).join('\n');
    expect(collapsed.split('\n')[0]).toBe('  ⎿  Applied 1 of 2 changes');
    expect(collapsed).toMatch(/✓ edit src\/a\.ts \+30 -0/);
    expect(collapsed).toMatch(/… \+\d+ diff lines \(ctrl\+o to expand\)/);
    expect(collapsed).toMatch(/✗ write test\/a\.test\.ts: has not been read/);
    expect(collapsed).not.toContain('added line');
    const one = { content: [], details: { outcomes: [result.details.outcomes[0]], durationMs: 5 } };
    const oneCollapsed = tool!.renderResult(one, { expanded: false, isPartial: false }, theme, { lastComponent: undefined, isError: false }).render(120).join('\n').split('\n');
    expect(oneCollapsed[0]).toMatch(/^ {2}⎿ {2}✓ edit src\/a\.ts \+30 -0$/);
    expect(oneCollapsed).toHaveLength(1 + 6 + 1);
    expect(oneCollapsed.at(-1)).toMatch(/… \+\d+ lines \(ctrl\+o to expand\)/);
    const failed = tool!.renderResult({ content: [{ type: 'text', text: '1. FAILED edit src/a.ts: oldText not found' }] }, { expanded: false, isPartial: false }, theme, { lastComponent: undefined, isError: true }).render(120).join('\n');
    expect(failed).toBe('  ⎿  Error: edit src/a.ts: oldText not found');
    const expanded = tool!.renderResult(result, { expanded: true, isPartial: false }, theme, { lastComponent: undefined, isError: false }).render(120).join('\n');
    expect(expanded).toContain('added line 29');
    expect(expanded).not.toMatch(/diff lines/);
  });

  it('counts diff lines and sizes queries', () => {
    expect(diffStats('+1 a\n-2 b\n-3 c\n 4 d')).toEqual({ added: 1, removed: 2 });
    expect(querySize({ type: 'edit', edits: [{}] })).toBe(' · 1 edit');
    expect(querySize({ type: 'delete' })).toBe('');
  });
});

describe('bash safety', () => {
  it('names catastrophic commands and lets ordinary ones through', () => {
    for (const command of ['rm -rf /', 'sudo rm -fr /*', 'cd x && rm -r -f ~/', 'rm --recursive --force "$HOME"', 'rm -rf --no-preserve-root /', 'mkfs.ext4 /dev/sda1', 'echo ok; shutdown -h now', 'dd if=/dev/zero of=/dev/disk2', ':(){ :|:& };:', 'x=$(reboot)', 'echo "$(rm -rf /)"', 'echo "now `shutdown -h now`"', 'echo "a $(echo "$(mkfs.ext4 /dev/sda)")"']) {
      expect(catastrophicCommand(command), command).toBeDefined();
    }
    for (const command of ['rm -rf dist', 'rm -rf ./build/', 'rm -rf /tmp/octocode-test', 'rm ~/notes.txt', 'echo "rm -rf /"', 'git commit -m "reboot the tests"', 'dd if=a of=/dev/null', 'yarn build']) {
      expect(catastrophicCommand(command), command).toBeUndefined();
    }
  });

  it('blocks catastrophic bash calls without touching the environment', async () => {
    const call = (command: string) => bashSafetyGate({ toolName: 'bash', input: { command } } as never);
    expect(await call('rm -rf ~')).toMatchObject({ block: true, reason: expect.stringMatching(/Refused: recursive removal of ~/) });
    expect(await call('ls -la')).toBeUndefined();
    expect(await bashSafetyGate({ toolName: 'file', input: { command: 'rm -rf /' } } as never)).toBeUndefined();
  });

  it('ignores quoted text and heredoc bodies, checks dd per command and sees past sudo options', () => {
    for (const command of [
      'rm -rf ./dist',
      'rm -rf node_modules',
      'dd if=x of=/dev/null',
      'echo "rm -rf /"',
      'grep shutdown src',
      'git commit -m "fix: close the socket (shutdown path)"',
      "git commit -m 'reboot; halt (later)'",
      'cat > notes.md <<\'EOF\'\ngrep -rn "of=/dev/sda" docs\nrm -rf /\nEOF',
      'cat <<-EOF > x.sh\n\tshutdown -h now\n\tEOF\necho done',
      'grep -rn "of=/dev/sda" docs',
      'echo ":(){ :|:& };:"',
    ]) {
      expect(catastrophicCommand(command), command).toBeUndefined();
    }
    for (const command of [
      'rm -rf /',
      'rm -rf ~',
      'sudo rm -rf /',
      'sudo -u root rm -rf /',
      'doas -u root rm -rf /',
      'x && rm -rf $HOME',
      'rm -rf "/"',
      'mkfs.ext4 /dev/sda',
      'dd if=/dev/zero of=/dev/sda',
      'sudo dd if=/dev/zero of="/dev/sda" bs=1M',
      'cat <<EOF > x\nbody\nEOF\nrm -rf /',
      'echo "a (b"; reboot',
    ]) {
      expect(catastrophicCommand(command), command).toBeDefined();
    }
  });
});
