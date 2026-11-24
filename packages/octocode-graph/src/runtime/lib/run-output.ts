// The exclusive log creation is the claim shared by live and headless writers.
import {
  closeSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  openSync,
  readSync,
  writeFileSync,
} from 'node:fs';
import { basename, join, resolve } from 'node:path';

/**
 * A saved run's header, the first line of its events.jsonl, read without the rest of the log; null for a directory that
 * is no saved run or has no readable header.
 */
export function runHeader(
  dir: string
): { root?: unknown; workspace?: unknown } | null {
  let fd: number | undefined;
  try {
    fd = openSync(join(dir, 'events.jsonl'), 'r');
    const head = Buffer.alloc(64 * 1024);
    const text = head.toString(
      'utf8',
      0,
      readSync(fd, head, 0, head.length, 0)
    );
    const first: unknown = JSON.parse(text.slice(0, text.indexOf('\n')));
    return first && typeof first === 'object' ? first : null;
  } catch {
    return null;
  } finally {
    if (fd !== undefined) closeSync(fd);
  }
}

/** The fields of a logged event the header check reads: every event's sequence number, a `run` header's project. */
export interface LoggedEvent {
  type: string;
  seq: number;
  root?: string;
  options?: { file?: string };
}

export function beginRun(out: string) {
  mkdirSync(out, { recursive: true });
  const occupied = () =>
    new Error(
      `Graph output already exists in ${out}. Choose a new output directory or reopen the saved run with --replay.`
    );
  if (
    ['data.json', 'stitched.json', 'server.json', 'chunks'].some(name =>
      existsSync(join(out, name))
    )
  )
    throw occupied();
  try {
    writeFileSync(join(out, 'events.jsonl'), '', { flag: 'wx' });
  } catch (error) {
    if (error instanceof Error && 'code' in error && error.code === 'EEXIST')
      throw occupied();
    throw error;
  }
}

// A fresh result directory under the workspace's .octocode/octocode-graph/.
export function freshOut(root: string) {
  const parent = resolve('.octocode/octocode-graph');
  mkdirSync(parent, { recursive: true });
  return mkdtempSync(
    join(parent, `${new Date().toISOString().slice(0, 10)}-${basename(root)}-`)
  );
}

// A refresh may append another header only for the same project, continuing the original sequence.
export function checkRunHeaders(directory: string) {
  let root: string | undefined,
    file: string | undefined,
    seenHeader = false,
    previous: number | undefined;
  return (event: LoggedEvent) => {
    if (event.type === 'run') {
      if (
        (seenHeader && (event.root !== root || event.options?.file !== file)) ||
        (previous !== undefined && event.seq <= previous)
      ) {
        throw new Error(
          `The event log in ${directory} contains multiple runs. Rebuild into a new output directory.`
        );
      }
      root = event.root;
      file = event.options?.file;
      seenHeader = true;
    }
    previous = event.seq;
    return root;
  };
}
