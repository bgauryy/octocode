/** Immutable workspace documents and bounded, scoped context discovery. */
import { createHash, randomBytes } from 'node:crypto';
import { closeSync, constants, fstatSync, fsyncSync, linkSync, lstatSync, mkdirSync, openSync, readSync, statSync, unlinkSync, writeSync } from 'node:fs';
import { isAbsolute, join, normalize } from 'node:path';
import * as catalog from './catalog.js';
import { query, readTransaction, rowInteger, rowOptionalText, rowText, transaction } from './database.js';
import type { CommandResults, ContextInput, DocumentData, ReadDocumentInput, ShareDocumentInput } from './catalog-types.generated.js';
import { leaseKind } from './rows.js';
import { ValidationError } from './errors.js';
import { overlap, resolvePath } from './paths.js';
import { append } from './records.js';
import { isWithin, relativePath, type StoreHost } from './store-types.js';
import { isBlank } from './text.js';
import { DAY_MS, now } from './timing.js';
import { asObject, jsonEqual, parseObject, type JsonObject } from './validation.js';
import { compact, decodeUtf8, dumps } from './json-text.js';

const MAX_BYTES = 1024 * 1024;
const CONTEXT_SCAN = 200;
const NAME = /^[a-z0-9][a-z0-9._-]{0,127}$/;

function lexists(path: string): boolean {
  try {
    lstatSync(path);
    return true;
  } catch {
    return false;
  }
}

function isFile(path: string): boolean {
  try {
    return statSync(path).isFile();
  } catch {
    return false;
  }
}

function scopePath(workspace: string, path: string): string {
  const resolved = resolvePath(workspace, path);
  if (!isWithin(resolved, workspace)) throw new ValidationError('Context path must stay inside this workspace');
  return relativePath(resolved, workspace);
}

type DocumentContext = NonNullable<DocumentData['context']> & { ttlMs: number };
type ContextScope = NonNullable<DocumentData['context']>;

/** The `context` of a share_document input with its path scoped to the workspace and its defaults filled; null when absent. */
function contextMetadata(workspace: string, value: ShareDocumentInput): DocumentContext | null {
  if (value.context === undefined) return null;
  const context = structuredClone(value.context);
  if (isBlank(String(context.summary ?? ''))) throw new ValidationError('Context summary must explain a useful fact or gotcha');
  const path = scopePath(workspace, context.path ?? '.');
  return { ...context, path, kind: context.kind ?? (isFile(join(workspace, path)) ? 'file' : 'tree'), ttlMs: context.ttlMs ?? DAY_MS };
}

function documentName(value: { readonly name: string }): string {
  const name = value.name;
  if (typeof name !== 'string' || !NAME.test(name) || name.includes('..')) throw new ValidationError("Use a lowercase document filename (letters, digits, dot, dash, underscore), without directories or '..'");
  return name;
}

function directory(workspace: string, create = false, coordination: string | null = null): string {
  let path = workspace;
  let components = ['.octocode', 'communication'];
  if (coordination !== null && coordination !== workspace) {
    path = coordination;
    components = ['octocode-communication'];
  }
  for (const component of components) {
    path = join(path, component);
    if (create) {
      try {
        mkdirSync(path);
      } catch (error) {
        if ((error as NodeJS.ErrnoException).code !== 'EEXIST') throw error;
      }
    }
    if (!lstatSync(path).isDirectory()) throw new ValidationError('Document directories must be real directories, not symlinks');
  }
  return path;
}

/** A read-only descriptor of a regular, non-symlink document file. */
function openDocument(path: string): number {
  if (!lstatSync(path).isFile()) throw new ValidationError('Document must be a regular file, not a symlink');
  const descriptor = openSync(path, constants.O_RDONLY | (constants.O_NOFOLLOW ?? 0) | (constants.O_NONBLOCK ?? 0));
  try {
    if (!fstatSync(descriptor).isFile()) throw new ValidationError('Document must be a regular file');
    return descriptor;
  } catch (error) {
    closeSync(descriptor);
    throw error;
  }
}

function readChunk(descriptor: number, size: number): Buffer {
  const buffer = Buffer.alloc(size);
  return buffer.subarray(0, readSync(descriptor, buffer, 0, size, null));
}

function readFile(path: string): string {
  const descriptor = openDocument(path);
  const chunks: Buffer[] = [];
  try {
    let total = 0;
    while (total <= MAX_BYTES) {
      const chunk = readChunk(descriptor, Math.min(65536, MAX_BYTES + 1 - total));
      if (!chunk.length) break;
      chunks.push(chunk);
      total += chunk.length;
    }
    if (total > MAX_BYTES) throw new ValidationError('Document exceeds 1 MiB');
    return decodeUtf8(Buffer.concat(chunks));
  } finally {
    closeSync(descriptor);
  }
}

/** The text field `key` of a stored document record. */
function stored(record: JsonObject, key: string): string {
  const value = record[key];
  if (typeof value !== 'string') throw new TypeError(`Stored document field ${key} is not text`);
  return value;
}

const optionalText = (record: JsonObject, key: string): string | undefined => (typeof record[key] === 'string' ? record[key] : undefined);
const optionalNumber = (record: JsonObject, key: string): number | undefined => (typeof record[key] === 'number' ? record[key] : undefined);

/** The scope of a stored document, field by field. */
function contextScope(record: JsonObject): ContextScope {
  const context = asObject(record, 'document context');
  const kind = optionalText(context, 'kind');
  const path = optionalText(context, 'path'), branch = optionalText(context, 'branch'), ttlMs = optionalNumber(context, 'ttlMs'), expiresAt = optionalNumber(context, 'expiresAt');
  // An absent field stays absent: scopes are compared as JSON.
  return {
    summary: stored(context, 'summary'), ...(path === undefined ? {} : { path }), ...(kind === undefined ? {} : { kind: leaseKind(kind) }),
    ...(branch === undefined ? {} : { branch }), ...(ttlMs === undefined ? {} : { ttlMs }), ...(expiresAt === undefined ? {} : { expiresAt }),
  };
}

/** What a document record says about its scope: its name, author and optional context. `context` lists these without the stored file's size or digest. */
function announcement(record: JsonObject): Pick<DocumentData, 'name' | 'author' | 'context'> {
  const context = record.context === undefined ? undefined : contextScope(asObject(record.context, 'document context'));
  return { name: stored(record, 'name'), author: stored(record, 'author'), ...(context === undefined ? {} : { context }) };
}

/** The document record a `document` row stores (written by `shareDocument`, so every field is present). */
function documentData(record: JsonObject): DocumentData {
  const bytes = optionalNumber(record, 'bytes');
  if (bytes === undefined) throw new TypeError('Stored document field bytes is not a number');
  const { name, author, context } = announcement(record);
  return {
    name, path: stored(record, 'path'), author, reasoning: stored(record, 'reasoning'), bytes, sha256: stored(record, 'sha256'),
    ...(context === undefined ? {} : { context }), ...(record.supersedes === undefined ? {} : { supersedes: stored(record, 'supersedes') }),
  };
}

export function documentRecord(host: StoreHost, name: string): DocumentData | null {
  const rows = query(host.db, 'SELECT a.data,d.workspace FROM documents d JOIN records a ON a.id=d.id WHERE d.workspace IN (SELECT value FROM json_each(?)) AND d.name=?', [dumps(host.sharedWorkspaces()), name]);
  // share_document keeps one name per repository; only a nonconforming SQL client can break it.
  if (rows.length > 1) throw new ValidationError(`Document name ${name} is registered in several worktrees: ${compact(rows.map(row => rowText(row, 'workspace')))}`);
  return rows.length ? documentData(parseObject(rowText(rows[0]!, 'data'), 'document record')) : null;
}

function documentPath(host: StoreHost, record: DocumentData): string {
  const origin = host.peer(record.author).workspace;
  const path = isAbsolute(record.path) ? record.path : join(origin, record.path);
  const stored = join(directory(origin, false, host.coordinationScope), record.name);
  if (normalize(path) !== normalize(stored)) throw new ValidationError('Document path is outside its registered storage');
  return stored;
}

const sha256 = (data: Uint8Array): string => createHash('sha256').update(data).digest('hex');

function sameDocument(record: DocumentData, path: string, content: string, metadata: DocumentContext | null, reasoning: string, supersedes: string | undefined): CommandResults['share_document'] {
  const { expiresAt: _expiresAt, ...scope } = record.context ?? { summary: '' };
  if (record.sha256 !== sha256(Buffer.from(content, 'utf8')) || readFile(path) !== content || !jsonEqual(record.context === undefined ? null : scope, metadata) || record.reasoning !== reasoning || record.supersedes !== supersedes)
    throw new ValidationError('Document is immutable or has changed on disk; publish a new name');
  return { created: false, document: record };
}

export function shareDocument(host: StoreHost, session: string, value: ShareDocumentInput): CommandResults['share_document'] {
  const name = documentName(value);
  const reasoning = catalog.text(value, 'reasoning');
  const content = value.content;
  if (typeof content !== 'string') throw new ValidationError('Document content required');
  const data = Buffer.from(content, 'utf8');
  if (data.length > MAX_BYTES) throw new ValidationError('Document exceeds 1 MiB');
  const metadata = contextMetadata(host.workspace, value);
  const supersedes = value.supersedes;
  if (supersedes === name) throw new ValidationError('A document cannot supersede itself');
  const folder = directory(host.workspace, true, host.coordinationScope);
  const path = join(folder, name);
  let record = readTransaction(host.db, () => {
    host.known(session, true);
    const found = documentRecord(host, name);
    if (found === null && supersedes !== undefined && documentRecord(host, supersedes) === null) throw new ValidationError('supersedes must name a published document: ' + supersedes);
    return found;
  });
  if (record !== null) return sameDocument(record, documentPath(host, record), content, metadata, reasoning, supersedes);
  if (lexists(path)) throw new ValidationError('Unregistered document already exists; preserve it and publish a new name');
  const temp = join(folder, '.document-' + randomBytes(6).toString('hex'));
  const descriptor = openSync(temp, 'wx', 0o600);
  try {
    try {
      for (let written = 0; written < data.length;) written += writeSync(descriptor, data, written);
      fsyncSync(descriptor);
    } finally {
      closeSync(descriptor);
    }
    let linked = false;
    try {
      return transaction(host.db, () => {
        host.known(session, true);
        record = documentRecord(host, name);
        if (record !== null) return sameDocument(record, documentPath(host, record), content, metadata, reasoning, supersedes);
        if (lexists(path)) throw new ValidationError('Unregistered document already exists; preserve it and publish a new name');
        // Same-directory link publication never exposes a partial file or overwrites.
        linkSync(temp, path);
        linked = true;
        const created: DocumentData = { name, path: host.coordinationScope !== host.workspace ? path : '.octocode/communication/' + name, author: session, reasoning, bytes: data.length, sha256: sha256(data) };
        if (supersedes !== undefined) created.supersedes = supersedes;
        if (metadata !== null) {
          created.context = { ...metadata, expiresAt: now() + metadata.ttlMs };
        }
        append(host.db, session, 'document', created, now(), name, name);
        return { created: true, document: created };
      });
    } catch (error) {
      // This call made the link; a rolled-back publication must not leave an unregistered file that blocks the name.
      if (linked) try { unlinkSync(path); } catch { /* already removed */ }
      throw error;
    }
  } finally {
    unlinkSync(temp);
  }
}

export function context(host: StoreHost, session: string, value: ContextInput): CommandResults['context'] {
  const path = scopePath(host.workspace, value.path ?? '.');
  const after = value.after ?? 0;
  const limit = value.limit ?? 10;
  return readTransaction(host.db, () => {
    host.known(session, true);
    const workspaces = dumps(host.sharedWorkspaces());
    const through = value.through ?? host.db.integer('SELECT coalesce(max(id),0) FROM documents WHERE workspace IN (SELECT value FROM json_each(?))', [workspaces]);
    if (through < after) throw new ValidationError('Context through must be at least after');
    const rows = query(host.db, 'SELECT d.id,d.workspace,a.branch,a.data FROM documents d JOIN records a ON a.id=d.id WHERE d.workspace IN (SELECT value FROM json_each(?)) AND d.id>? AND d.id<=? ORDER BY d.id LIMIT ?', [workspaces, after, through, CONTEXT_SCAN]);
    const items: CommandResults['context']['items'] = [];
    let cursor = after;
    let scanned = 0;
    let size = 0;
    const at = now();
    for (const row of rows) {
      const previous = cursor;
      cursor = rowInteger(row, 'id');
      scanned += 1;
      const record = announcement(parseObject(rowText(row, 'data'), 'document record'));
      const scope = record.context;
      if (scope?.path === undefined || (scope.expiresAt ?? 0) <= at || (scope.branch !== undefined && scope.branch !== value.branch) || !overlap(scope.path, scope.kind ?? 'tree', path, 'file')) continue;
      const item = { id: cursor, name: record.name, author: record.author, workspace: rowText(row, 'workspace'), branch: rowOptionalText(row, 'branch'), context: scope };
      const amount = Buffer.byteLength(compact(item));
      if (items.length && size + amount > catalog.PAGE_BYTES) {
        cursor = previous;
        scanned -= 1;
        break;
      }
      size += amount;
      items.push(item);
      if (items.length === limit) break;
    }
    if (scanned === rows.length && rows.length < CONTEXT_SCAN) cursor = through;
    return { items, cursor, scanned, ...(cursor < through ? { next: { command: 'context', input: { ...value, after: cursor, through } } } : {}) };
  });
}

/** UTF-8 decoding where only a character cut off at the end is tolerated (and dropped). */
function decodePage(page: Buffer): string {
  try {
    return decodeUtf8(page);
  } catch {
    /* fall through to the cut-off check */
  }
  for (let cut = 1; cut <= 3 && cut <= page.length; cut += 1) {
    const lead = page[page.length - cut]!;
    if ((lead & 0xc0) === 0x80) continue;
    const need = lead >= 0xf0 ? 4 : lead >= 0xe0 ? 3 : lead >= 0xc0 ? 2 : 1;
    if (need > cut) {
      // The tail must be a valid prefix of a character: some continuation fills it into one.
      const tail = page.subarray(page.length - cut);
      const completes = [0x80, 0xbf].some(fill => {
        try {
          decodeUtf8(Buffer.concat([tail, Buffer.alloc(need - cut, fill)]));
          return true;
        } catch {
          return false;
        }
      });
      if (completes) {
        try {
          return decodeUtf8(page.subarray(0, page.length - cut));
        } catch {
          /* an earlier invalid byte */
        }
      }
    }
    break;
  }
  throw new ValidationError('Offset must be a UTF-8 byte boundary within the document');
}

export function readDocument(host: StoreHost, session: string, value: ReadDocumentInput): CommandResults['read_document'] {
  host.known(session, true);
  const name = documentName(value);
  const record = documentRecord(host, name);
  if (record === null) {
    const candidates = query(host.db, 'SELECT name FROM documents WHERE workspace IN (SELECT value FROM json_each(?)) AND name>=? AND name<? ORDER BY name LIMIT 5', [dumps(host.sharedWorkspaces()), name + '.', name + '/']);
    throw new ValidationError(`Unknown document '${name}' in this repository. Use the exact published document.name, including its extension. Matching names (up to 5): ${compact(candidates.map(row => rowText(row, 'name')))}. Read the document successfully before using its contents.`);
  }
  const offset = value.offset ?? 0;
  const limit = value.limit ?? 8192;
  const length = record.bytes;
  if (!Number.isInteger(length) || length < 0 || length > MAX_BYTES) throw new ValidationError('Invalid document size');
  if (offset > length) throw new ValidationError('Offset must be a UTF-8 byte boundary within the document');
  const digest = createHash('sha256');
  const page: Buffer[] = [];
  let scanned = 0;
  const descriptor = openDocument(documentPath(host, record));
  try {
    while (scanned <= MAX_BYTES) {
      const data = readChunk(descriptor, Math.min(8192, MAX_BYTES + 1 - scanned));
      if (!data.length) break;
      digest.update(data);
      const start = Math.min(Math.max(offset - scanned, 0), data.length);
      const end = Math.min(Math.max(offset + limit - scanned, 0), data.length);
      page.push(data.subarray(start, end));
      scanned += data.length;
    }
  } finally {
    closeSync(descriptor);
  }
  if (scanned !== length || record.sha256 !== digest.digest('hex')) throw new ValidationError('Document integrity mismatch; ask the author to publish a new document');
  const content = decodePage(Buffer.concat(page));
  const end = offset + Buffer.byteLength(content);
  return { document: record, offset, content, ...(end < length ? { next: { command: 'read_document', input: { ...value, name, offset: end, limit } } } : {}) };
}
