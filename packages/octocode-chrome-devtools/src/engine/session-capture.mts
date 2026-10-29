// Pages, document epochs, immutable captures and journals inside a durable
// session (see session-store.mts). Captures are staged privately and published
// by one directory rename; a page manifest names only published captures.
import {
  appendFileSync,
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  renameSync,
  rmSync,
  statSync,
} from 'node:fs';
import { createHash } from 'node:crypto';
import { join } from 'node:path';
import { errorMessage } from './error-message.mjs';
import {
  assertId,
  isRecord,
  listSessions,
  newId,
  openSession,
  readJson,
  readLease,
  sessionDir,
  updateSession,
  writeAtomic,
} from './session-store.mjs';

/** A session in a workspace. */
export type SessionRef = { workspace: string; session: string };
/** One document epoch of one page in a session. */
export type DocumentRef = SessionRef & { page: string; document: string };

export type PageManifest = {
  id: string;
  targets: { targetId: string; attachedAt: string }[];
  opener: string | null;
  documents: { id: string; url: string; startedAt: string }[];
  captures: { id: string; document: string; kind: string }[];
};

const isString = (value: unknown): value is string => typeof value === 'string';

function isPageManifest(value: unknown): value is PageManifest {
  return (
    isRecord(value) &&
    isString(value.id) &&
    (value.opener === null || isString(value.opener)) &&
    Array.isArray(value.targets) &&
    value.targets.every(
      row => isRecord(row) && isString(row.targetId) && isString(row.attachedAt)
    ) &&
    Array.isArray(value.documents) &&
    value.documents.every(
      row =>
        isRecord(row) &&
        isString(row.id) &&
        isString(row.url) &&
        isString(row.startedAt)
    ) &&
    Array.isArray(value.captures) &&
    value.captures.every(
      row =>
        isRecord(row) &&
        isString(row.id) &&
        isString(row.document) &&
        isString(row.kind)
    )
  );
}

function pageDir({ workspace, session }: SessionRef, page: string): string {
  return join(sessionDir(workspace, session), assertId(page, 'p'));
}

function readPage(ref: SessionRef, page: string): PageManifest {
  const value = readJson(join(pageDir(ref, page), 'page.json'));
  if (!isPageManifest(value) || value.id !== page)
    throw Error(`page ${page} manifest is malformed`);
  return value;
}

function writePage(ref: SessionRef, value: PageManifest): void {
  writeAtomic(
    join(pageDir(ref, value.id), 'page.json'),
    JSON.stringify(value, null, 2)
  );
}

function capturesDir(at: DocumentRef): string {
  return join(
    pageDir(at, at.page),
    'documents',
    assertId(at.document, 'd'),
    'captures'
  );
}

/** A new logical page (tab) in the session, optionally opened by another. */
export function createPage(
  ref: SessionRef,
  targetId: string,
  opener: string | null = null,
  now = new Date()
): PageManifest {
  if (opener !== null) assertId(opener, 'p');
  const page: PageManifest = {
    id: newId('p'),
    targets: [{ targetId, attachedAt: now.toISOString() }],
    opener,
    documents: [],
    captures: [],
  };
  mkdirSync(join(pageDir(ref, page.id), 'documents'), {
    recursive: true,
    mode: 0o700,
  });
  writePage(ref, page);
  updateSession(ref.workspace, ref.session, manifest =>
    manifest.pages.push(page.id)
  );
  return page;
}

/**
 * Starts a document epoch: a top-level navigation or context replacement
 * invalidates references taken in the previous one.
 */
export function beginDocument(
  ref: SessionRef,
  page: string,
  url: string,
  now = new Date()
): string {
  const value = readPage(ref, page);
  const id = newId('d');
  mkdirSync(join(pageDir(ref, page), 'documents', id), { mode: 0o700 });
  value.documents.push({ id, url, startedAt: now.toISOString() });
  writePage(ref, value);
  return id;
}

export type CaptureFile = { name: string; bytes: () => string | Uint8Array };
export type CaptureInput = {
  kind: string;
  sourceUrl: string;
  files: CaptureFile[];
  complete: boolean;
  limit?: string;
};
export type CaptureManifest = {
  id: string;
  page: string;
  document: string;
  kind: string;
  sourceUrl: string;
  capturedAt: string;
  complete: boolean;
  limit: string | null;
  files: { name: string; sha256: string; byteLength: number }[];
};

const FILE = /^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$/;

function isCaptureManifest(value: unknown): value is CaptureManifest {
  return (
    isRecord(value) &&
    isString(value.id) &&
    isString(value.page) &&
    isString(value.document) &&
    isString(value.kind) &&
    isString(value.sourceUrl) &&
    isString(value.capturedAt) &&
    typeof value.complete === 'boolean' &&
    (value.limit === null || isString(value.limit)) &&
    Array.isArray(value.files) &&
    value.files.every(
      row =>
        isRecord(row) &&
        isString(row.name) &&
        FILE.test(row.name) &&
        isString(row.sha256) &&
        typeof row.byteLength === 'number'
    )
  );
}

/**
 * Writes every file into a private staging folder, then publishes it by
 * rename and records it on the page. A failure leaves no published capture
 * and the page manifest unchanged; the staging folder is removed.
 */
export function publishCapture(
  at: DocumentRef,
  capture: CaptureInput,
  now = new Date()
): CaptureManifest {
  const value = readPage(at, at.page);
  if (!value.documents.some(row => row.id === at.document))
    throw Error(`document ${at.document} is not an epoch of page ${at.page}`);
  const captures = capturesDir(at);
  mkdirSync(captures, { recursive: true, mode: 0o700 });
  const id = newId('c');
  const staging = join(captures, `.staging-${id}`);
  mkdirSync(staging, { mode: 0o700 });
  try {
    const files = capture.files.map(file => {
      if (!FILE.test(file.name) || file.name === 'manifest.json')
        throw Error(`invalid capture file name: ${JSON.stringify(file.name)}`);
      const bytes = file.bytes();
      writeAtomic(join(staging, file.name), bytes);
      const buffer = typeof bytes === 'string' ? Buffer.from(bytes) : bytes;
      return {
        name: file.name,
        sha256: createHash('sha256').update(buffer).digest('hex'),
        byteLength: buffer.byteLength,
      };
    });
    const manifest: CaptureManifest = {
      id,
      page: at.page,
      document: at.document,
      kind: capture.kind,
      sourceUrl: capture.sourceUrl,
      capturedAt: now.toISOString(),
      complete: capture.complete,
      limit: capture.limit ?? null,
      files,
    };
    writeAtomic(
      join(staging, 'manifest.json'),
      JSON.stringify(manifest, null, 2)
    );
    renameSync(staging, join(captures, id));
    value.captures.push({ id, document: at.document, kind: capture.kind });
    writePage(at, value);
    return manifest;
  } catch (error) {
    rmSync(staging, { recursive: true, force: true });
    throw error;
  }
}

/** A published capture, with every file checked against its manifest. */
export function readCapture(at: DocumentRef, capture: string): CaptureManifest {
  const dir = join(capturesDir(at), assertId(capture, 'c'));
  const manifest = readJson(join(dir, 'manifest.json'));
  if (!isCaptureManifest(manifest) || manifest.id !== capture)
    throw Error(`capture ${capture} manifest is malformed`);
  for (const file of manifest.files) {
    const bytes = readFileSync(join(dir, file.name));
    const digest = createHash('sha256').update(bytes).digest('hex');
    if (digest !== file.sha256 || bytes.byteLength !== file.byteLength)
      throw Error(
        `capture ${capture} file ${file.name} does not match its manifest`
      );
  }
  return manifest;
}

/** Staging folders left by an interrupted capture, for cleanup reporting. */
export function abandonedStaging(at: DocumentRef): string[] {
  const captures = capturesDir(at);
  if (!existsSync(captures)) return [];
  return readdirSync(captures).filter(name => name.startsWith('.staging-'));
}

/** Appends one record with the next sequence number to a JSONL journal. */
export function appendJournal(path: string, record: object): number {
  const seq = readJournal(path).records.length;
  appendFileSync(path, `${JSON.stringify({ seq, ...record })}\n`, {
    mode: 0o600,
  });
  return seq;
}

/**
 * Complete records, and the byte offset of an incomplete final record when
 * a writer stopped mid-line: the gap is reported, never silently dropped.
 */
export function readJournal(path: string): {
  records: Record<string, unknown>[];
  partialAt: number | null;
} {
  if (!existsSync(path)) return { records: [], partialAt: null };
  const text = readFileSync(path, 'utf8');
  const end = text.lastIndexOf('\n') + 1;
  const records = text
    .slice(0, end)
    .split('\n')
    .filter(line => line !== '')
    .map((line, index) => {
      const value: unknown = JSON.parse(line);
      if (!isRecord(value))
        throw Error(`journal ${path} record ${index} is not an object`);
      return value;
    });
  return {
    records,
    partialAt: end < text.length ? Buffer.byteLength(text.slice(0, end)) : null,
  };
}

export type PrunePlan = {
  remove: { session: string; reason: string }[];
  keep: { session: string; reason: string }[];
};

/**
 * What pruning would remove: closed, unleased sessions older than
 * `maxAgeMs`. Active or leased sessions are kept, each with its reason.
 * Planning deletes nothing.
 */
export function planPrune(
  workspace: string,
  maxAgeMs: number,
  now = Date.now()
): PrunePlan {
  const plan: PrunePlan = { remove: [], keep: [] };
  for (const session of listSessions(workspace)) {
    let manifest;
    try {
      manifest = openSession(workspace, session);
    } catch (error) {
      plan.keep.push({
        session,
        reason: `unreadable manifest: ${errorMessage(error)}`,
      });
      continue;
    }
    const lease = readLease(workspace, session);
    if (lease && !lease.stale)
      plan.keep.push({ session, reason: `leased by pid ${lease.lease.pid}` });
    else if (manifest.lifecycle === 'active')
      plan.keep.push({ session, reason: 'active' });
    else if (now - Date.parse(manifest.updatedAt) < maxAgeMs)
      plan.keep.push({ session, reason: 'recent' });
    else plan.remove.push({ session, reason: 'closed and expired' });
  }
  return plan;
}

/** Removes exactly the sessions a plan lists, rechecking each lease first. */
export function applyPrune(workspace: string, plan: PrunePlan): string[] {
  const removed = [];
  for (const { session } of plan.remove) {
    const lease = readLease(workspace, session);
    if (lease && !lease.stale) continue;
    const dir = sessionDir(workspace, session);
    if (existsSync(dir) && statSync(dir).isDirectory()) {
      rmSync(dir, { recursive: true, force: true });
      removed.push(session);
    }
  }
  return removed;
}
