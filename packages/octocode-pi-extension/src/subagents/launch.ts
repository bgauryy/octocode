import { randomBytes } from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import type { ActionClass, PermissionMode } from '../permission/mode.js';
import { atomicWriteFileSync, shortHash } from '../shared/atomic.js';
import { PRIVATE_FILE_MODE, sessionOutputDir } from '../shared/home.js';
import { reportDir } from './run-output.js';

/** The child's place in the team: its name (id), its role (profile), its goal, and the agent that started it. */
export interface Identity {
  id: string;
  role?: string;
  parentId?: string;
  goal?: string;
  /** Works with its sibling subagents (messages them directly), not only for its parent. */
  collaborate?: boolean;
  /** The parent's team workspace, for a child whose cwd is an isolated worktree (a different git root). */
  workspace?: string;
  /** Set only when the parent's project config is trusted: the repository whose stored decision an isolated child follows. */
  trustRoot?: string;
  /** Its handoff folder: files it writes there are read by the parent on demand instead of riding in its report. */
  scratch?: string;
  /** The parent's Pi session id: the backlog claims the child may update. */
  parentSession?: string;
  /** The parent's permission mode now, and what its spawn ask granted: the child is never looser than its parent. */
  permission?: { mode: PermissionMode; grants: readonly ActionClass[] };
  /** The parent can ask a person: the child sends its approval dialogs to the parent (`OCTOCODE_PERMISSION_RELAY`) instead of denying them. */
  relay?: boolean;
}

/** The Pi CLI that runs this process, else the installed Pi package's CLI, else `pi` on PATH. */
export function piInvocation(argv = process.argv): { command: string; prefix: string[] } {
  const script = argv[1];
  if (script && /(^|[\\/])(pi|cli\.js)$/.test(script) && fs.existsSync(script)) return { command: process.execPath, prefix: [script] };
  try {
    const main = fileURLToPath(import.meta.resolve('@earendil-works/pi-coding-agent'));
    const cli = path.join(path.dirname(main), 'bundle', 'cli.js');
    if (fs.existsSync(cli)) return { command: process.execPath, prefix: [cli] };
  } catch {
    // Fall through to PATH lookup.
  }
  return { command: 'pi', prefix: [] };
}

/** This extension's entry file, so children load the same Octocode tools the parent has. */
export function selfExtensionPath(): string | undefined {
  // The entry (`index.js` in dist, `index.ts` in src) sits in the folder above this module's own.
  const dir = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
  return ['index.js', 'index.ts'].map((file) => path.join(dir, file)).find((file) => fs.existsSync(file));
}

/**
 * Writes a profile prompt to an owner-only file in `dir`, named by its content hash, and returns the path. Pi's
 * `--append-system-prompt` takes text or a path and reads an existing path as a file, so a prompt passed as text that
 * happens to name a file would be replaced by that file: a path it always reads is unambiguous.
 */
export function promptFile(prompt: string, dir: string): string {
  const file = path.join(dir, `system-prompt-${shortHash(prompt)}.md`);
  atomicWriteFileSync(file, prompt, { mode: 0o600 });
  return file;
}

/**
 * Writes `nonce` to a new owner-only file in this session's private folder and returns its path, or undefined when it
 * cannot be written (the child then runs without a secret: its headers and audit entries are not accepted, and it logs that).
 * The child reads the file once and unlinks it (`takeNonce`); `runSubagent` removes it at the end of the run otherwise.
 */
export function writeNonceFile(nonce: string, id: string): string | undefined {
  try {
    const file = path.join(sessionOutputDir('output'), `nonce-${id.replace(/[^\w.-]/g, '_')}-${randomBytes(8).toString('hex')}`);
    fs.writeFileSync(file, nonce, { flag: 'wx', mode: PRIVATE_FILE_MODE });
    return file;
  } catch {
    return undefined;
  }
}

/** Where a child saves its conversation: Pi's session folder and the session id (the child's name, so its Octocode folder maps back to it). */
export interface ChildSession {
  dir: string;
  id: string;
}

/**
 * Every child saves its conversation in its scratch folder (`<scratch>/session`, removed with the folder), under its
 * own name (no new child gets the name of an existing scratch folder, `scratchNames`, so none reopens another's): its compaction summary then names a transcript to read, and the parent can find it. Without a scratch
 * folder it goes to the run's private output folder. Whether the child stays reachable is `lingerSeconds`, not this.
 */
export function childSession(identity: Pick<Identity, 'id' | 'scratch'>): ChildSession {
  const root = identity.scratch ?? reportDir(identity.id);
  if (!root) throw new Error(`Subagent ${identity.id} has nowhere to save its session: neither its scratch folder nor the output folder can be written.`);
  return { dir: path.join(root, 'session'), id: identity.id };
}

/** The saved session file (`<timestamp>_<id>.jsonl`, named by Pi), once the child has written it. */
export function childSessionFile(session: ChildSession): string | undefined {
  try {
    const name = fs.readdirSync(session.dir).find((entry) => entry.endsWith(`_${session.id}.jsonl`));
    return name ? path.join(session.dir, name) : undefined;
  } catch {
    return undefined;
  }
}
