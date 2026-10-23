/** Bounded newline-delimited JSON frames and the owned vendor-process lifecycle. */
import { spawn, spawnSync, type ChildProcess } from 'node:child_process';
import { accessSync, closeSync, constants, openSync, readSync } from 'node:fs';
import { delimiter, join } from 'node:path';
import { ValidationError } from './errors.js';
import { dumps, encode, parseJsonBytes } from './json-text.js';
import { monotonic, type Stop } from './timing.js';

export const MAX_FRAME = 8 * 1024 * 1024;

function which(command: string): string | null {
  const names = process.platform === 'win32' ? ['', ...(process.env.PATHEXT ?? '.EXE;.CMD;.BAT').split(';')] : [''];
  const directories = command.includes('/') || command.includes('\\') ? [''] : (process.env.PATH ?? '').split(delimiter);
  for (const directory of directories) {
    for (const extension of names) {
      const candidate = directory ? join(directory, command + extension) : command + extension;
      try {
        accessSync(candidate, constants.X_OK);
        return candidate;
      } catch {
        /* next candidate */
      }
    }
  }
  return null;
}

/** Splits a line into words as a POSIX shell does: whitespace separates, quotes and backslashes group and escape. */
export function splitShellWords(line: string): string[] {
  const words: string[] = [];
  let word: string | null = null;
  for (let index = 0; index < line.length; index += 1) {
    const character = line[index]!;
    if (' \t\r\n'.includes(character)) {
      if (word !== null) words.push(word);
      word = null;
    } else if (character === "'") {
      const end = line.indexOf("'", index + 1);
      if (end < 0) throw new ValidationError('No closing quotation');
      word = (word ?? '') + line.slice(index + 1, end);
      index = end;
    } else if (character === '"') {
      let quoted = '';
      for (index += 1; ; index += 1) {
        if (index >= line.length) throw new ValidationError('No closing quotation');
        const inner = line[index]!;
        if (inner === '"') break;
        if (inner === '\\') {
          if (index + 1 >= line.length) throw new ValidationError('No closing quotation');
          const escaped = line[index + 1]!;
          if (escaped === '"' || escaped === '\\') {
            quoted += escaped;
            index += 1;
            continue;
          }
        }
        quoted += inner;
      }
      word = (word ?? '') + quoted;
    } else if (character === '\\') {
      if (index + 1 >= line.length) throw new ValidationError('No escaped character');
      word = (word ?? '') + line[(index += 1)];
    } else word = (word ?? '') + character;
  }
  if (word !== null) words.push(word);
  return words;
}

/** Invoke script interpreters explicitly, preserving their declared arguments. */
export function executableArgv(command: string, args: readonly string[]): string[] {
  const path = which(command);
  if (path) {
    const head = Buffer.alloc(4096);
    let length: number;
    try {
      const descriptor = openSync(path, 'r');
      try {
        length = readSync(descriptor, head, 0, head.length, null);
      } finally {
        closeSync(descriptor);
      }
    } catch {
      return [command, ...args];
    }
    const newline = head.subarray(0, length).indexOf(10);
    if (head[0] === 0x23 && head[1] === 0x21 && newline >= 0) {
      const interpreter = splitShellWords(head.subarray(2, newline + 1).toString('utf8').trim());
      if (interpreter.length) return [...interpreter, path, ...args];
    }
  }
  return [command, ...args];
}

export class OversizedFrame extends ValidationError {
  override name = 'OversizedFrame';
}

export function refusal(identity: unknown): Record<string, unknown> {
  return { id: identity, error: { code: -32601, message: 'Communication delivery cannot approve actions or execute tools' } };
}

const MAX_BUFFERED_FRAMES = 128;
const MAX_PENDING = 512;
const WRITE_TIMEOUT_MS = 30000;
/** How long a vendor process group gets after SIGTERM before SIGKILL; the parent-death watchdog uses the same grace. */
export const TERMINATE_GRACE_MS = 2000;

type Message = Record<string, unknown>;

/** A newline-delimited JSON frame reader that never buffers more than one frame beyond MAX_FRAME. */
class FrameReader {
  private chunks: Buffer = Buffer.alloc(0);
  private ended = false;
  readonly frames: (Message | Error)[] = [];
  private failed = false;
  private waiter: (() => void) | null = null;

  constructor(private readonly stream: NodeJS.ReadableStream) {
    stream.on('data', (chunk: Buffer) => {
      this.chunks = this.chunks.length ? Buffer.concat([this.chunks, chunk]) : chunk;
      this.pump();
    });
    stream.on('end', () => {
      this.ended = true;
      this.pump();
    });
    stream.on('error', (error: Error) => this.fail(error));
  }

  private fail(error: Error): void {
    if (this.failed) return;
    this.failed = true;
    this.frames.push(error);
    this.chunks = Buffer.alloc(0);
    this.wake();
  }

  private wake(): void {
    const waiter = this.waiter;
    this.waiter = null;
    waiter?.();
  }

  private line(): Buffer | null {
    const newline = this.chunks.indexOf(10);
    if (newline >= 0 && newline + 1 <= MAX_FRAME) {
      const data = this.chunks.subarray(0, newline + 1);
      this.chunks = this.chunks.subarray(newline + 1);
      return data;
    }
    if (this.chunks.length > MAX_FRAME) throw new OversizedFrame('JSON frame exceeds 8 MiB');
    if (this.ended && this.chunks.length) {
      const data = this.chunks;
      this.chunks = Buffer.alloc(0);
      return data;
    }
    return null;
  }

  private pump(): void {
    try {
      while (!this.failed && this.frames.length < MAX_BUFFERED_FRAMES) {
        const data = this.line();
        if (data === null) break;
        this.frames.push(parseJsonBytes(data) as Message);
      }
      if (!this.failed && this.ended && !this.chunks.length && this.frames.length < MAX_BUFFERED_FRAMES) this.fail(new Error('Vendor process closed stdout'));
    } catch (error) {
      this.fail(error as Error);
    }
    if (this.frames.length >= MAX_BUFFERED_FRAMES) this.stream.pause();
    this.wake();
  }

  /** The next frame or error, or undefined after `timeoutMs`. */
  async next(timeoutMs: number): Promise<Message | Error | undefined> {
    const end = monotonic() + timeoutMs;
    while (!this.frames.length) {
      const remaining = end - monotonic();
      if (remaining <= 0) break;
      await new Promise<void>(resolve => {
        const timer = setTimeout(resolve, remaining);
        this.waiter = () => {
          clearTimeout(timer);
          resolve();
        };
      });
      this.waiter = null;
    }
    const frame = this.frames.shift();
    if (this.frames.length < MAX_BUFFERED_FRAMES) {
      this.stream.resume();
      this.pump();
    }
    return frame;
  }
}

/** A vendor process speaking newline-delimited JSON on stdio; it is killed with its process group on close. */
export class Wire implements AsyncDisposable {
  readonly stop: Stop;
  readonly pending: Message[] = [];
  private sequence = 0;
  private stopped = false;
  private readonly child: ChildProcess;
  private readonly reader: FrameReader;
  private readonly exited: Promise<void>;
  private exitCode: number | null = null;

  constructor(command: string, args: readonly string[], cwd: string, environment: Iterable<readonly [string, string]> = [], private readonly deadline: number | null = null, stop?: Stop) {
    this.stop = stop ?? { aborted: false };
    const [file, ...rest] = executableArgv(command, args);
    this.child = spawn(file!, rest, { cwd, env: { ...process.env, ...Object.fromEntries(environment) }, stdio: ['pipe', 'pipe', 'inherit'], detached: process.platform !== 'win32' });
    this.reader = new FrameReader(this.child.stdout!);
    this.exited = new Promise(resolve => {
      this.child.once('exit', code => {
        this.exitCode = code ?? -1;
        resolve();
      });
      this.child.once('error', () => resolve());
    });
    this.child.stdin!.on('error', () => undefined);
  }

  get pid(): number | undefined {
    return this.child.pid;
  }

  private limit(): number {
    return Math.min(monotonic() + WRITE_TIMEOUT_MS, this.deadline ?? Infinity);
  }

  async send(value: unknown): Promise<void> {
    if (this.stopped) throw new Error('Vendor process closed');
    const frame = Buffer.concat([encode(value), Buffer.from('\n')]);
    if (frame.length > MAX_FRAME) throw new OversizedFrame('JSON frame exceeds 8 MiB');
    const limit = this.limit();
    await new Promise<void>((resolve, reject) => {
      const timer = setTimeout(() => finish(new Error('Timed out writing to vendor')), Math.max(0, limit - monotonic()));
      const poll = setInterval(() => this.stop.aborted && finish(new Error('Worker stopped')), 100);
      const finish = (error?: Error): void => {
        clearTimeout(timer);
        clearInterval(poll);
        if (error) reject(error);
        else resolve();
      };
      this.child.stdin!.write(frame, error => finish(error ?? undefined));
    });
  }

  /** The next frame within `timeoutMs`, null on timeout; a reader failure is thrown. */
  async receive(timeoutMs: number): Promise<Message | null> {
    const value = await this.reader.next(timeoutMs);
    if (value instanceof Error) throw value;
    return value ?? null;
  }

  async event(timeoutMs: number): Promise<Message | null> {
    return this.pending.shift() ?? (await this.receive(timeoutMs));
  }

  request(method: string, params: Message): Promise<unknown> {
    return this.exchange(method, params, false);
  }

  piRequest(method: string, params: Message): Promise<unknown> {
    return this.exchange(method, params, true);
  }

  private async exchange(method: string, params: Message, pi: boolean): Promise<unknown> {
    this.sequence += 1;
    const identity = pi ? String(this.sequence) : this.sequence;
    await this.send(pi ? { ...params, id: identity, type: method } : { id: identity, method, params });
    const limit = this.limit();
    while (monotonic() < limit) {
      if (this.stop.aborted) throw new Error('Worker stopped');
      const value = await this.receive(Math.min(100, limit - monotonic()));
      if (value === null) continue;
      if (typeof value !== 'object' || Array.isArray(value)) throw new TypeError('Vendor sent a frame that is not a JSON object');
      if (value.id === identity && !('method' in value)) {
        if ('error' in value || (pi && value.success !== true)) throw new Error('Vendor request failed: ' + dumps(value));
        return value[pi ? 'data' : 'result'];
      }
      if ('id' in value && 'method' in value) await this.send(refusal(value.id));
      else {
        if (this.pending.length >= MAX_PENDING) throw new Error('Vendor notification queue full');
        this.pending.push(value);
      }
    }
    throw new Error('Timed out: ' + method);
  }

  async close(): Promise<void> {
    if (this.stopped) return;
    this.stopped = true;
    if (this.exitCode === null && this.child.pid !== undefined) {
      if (process.platform === 'win32') spawnSync('taskkill', ['/PID', String(this.child.pid), '/T', '/F'], { stdio: 'ignore' });
      else {
        const signal = (name: NodeJS.Signals): void => {
          try {
            process.kill(-this.child.pid!, name);
          } catch {
            /* the group is already gone */
          }
        };
        signal('SIGTERM');
        const timer = new Promise<void>(resolve => setTimeout(resolve, TERMINATE_GRACE_MS).unref());
        await Promise.race([this.exited, timer]);
        if (this.exitCode === null) signal('SIGKILL');
      }
      if (this.exitCode === null) this.child.kill('SIGKILL');
    }
    await this.exited;
    this.child.stdin!.destroy();
    this.child.stdout!.destroy();
  }

  [Symbol.asyncDispose](): Promise<void> {
    return this.close();
  }
}
