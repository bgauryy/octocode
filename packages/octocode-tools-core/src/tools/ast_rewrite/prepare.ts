import { createHash } from 'node:crypto';
import { lstat, readFile, realpath } from 'node:fs/promises';
import { isAbsolute, relative, resolve, sep } from 'node:path';
import { TextDecoder } from 'node:util';
import type { AstGrepJsonMatch } from './types.js';
import { createUnifiedPatch } from './patch.js';
import { rewriteError as error } from './result.js';
import type {
  AstRewriteCapture,
  AstRewriteError,
  AstRewriteMatch,
  PreparedFile,
} from './types.js';

function hash(value: string | Buffer): string {
  return createHash('sha256').update(value).digest('hex');
}

export function isWithin(root: string, target: string): boolean {
  const relation = relative(root, target);
  return (
    relation === '' ||
    (!relation.startsWith(`..${sep}`) &&
      relation !== '..' &&
      !isAbsolute(relation))
  );
}

function portableRelative(root: string, target: string): string {
  return relative(root, target).split(sep).join('/');
}

function replacementRange(match: AstGrepJsonMatch): {
  start: number;
  end: number;
} {
  return match.replacementOffsets ?? match.range.byteOffset;
}

function validateUtf8(bytes: Buffer): boolean {
  try {
    new TextDecoder('utf-8', { fatal: true }).decode(bytes);
    return !bytes.includes(0);
  } catch {
    return false;
  }
}

function applyEdits(before: Buffer, matches: AstRewriteMatch[]): Buffer | null {
  const chunks: Buffer[] = [];
  let offset = 0;
  for (const match of matches) {
    const { start, end } = match.byteRange;
    if (start < offset || end < start || end > before.length) return null;
    const expected = Buffer.from(match.text);
    if (!before.subarray(start, end).equals(expected)) return null;
    chunks.push(before.subarray(offset, start), Buffer.from(match.replacement));
    offset = end;
  }
  chunks.push(before.subarray(offset));
  return Buffer.concat(chunks);
}

function matchId(
  path: string,
  beforeHash: string,
  source: AstGrepJsonMatch
): string {
  const range = replacementRange(source);
  return hash(
    JSON.stringify([
      path,
      beforeHash,
      range.start,
      range.end,
      source.text,
      source.replacement,
    ])
  );
}

async function resolveTarget(
  rawPath: string,
  realRoot: string,
  boundary: string,
  cwd: string
): Promise<
  { ok: true; path: string } | { ok: false; result: AstRewriteError }
> {
  const unresolved = isAbsolute(rawPath)
    ? resolve(rawPath)
    : resolve(cwd, rawPath);
  let actual: string;
  try {
    actual = await realpath(unresolved);
    const info = await lstat(unresolved);
    if (info.isSymbolicLink() || !info.isFile()) {
      return {
        ok: false,
        result: error(
          'ast.rewrite.symlink_target',
          'ast-grep returned a symlink or non-file target; no changes were prepared.',
          { details: { path: rawPath } }
        ),
      };
    }
  } catch {
    return {
      ok: false,
      result: error(
        'ast.rewrite.target_unavailable',
        'ast-grep returned a target that could not be verified.',
        { details: { path: rawPath } }
      ),
    };
  }
  if (!isWithin(boundary, actual)) {
    return {
      ok: false,
      result: error(
        'ast.rewrite.path_escape',
        'ast-grep returned a target outside the real requested root.',
        { details: { path: rawPath } }
      ),
    };
  }
  if (realRoot !== boundary && actual !== realRoot) {
    return {
      ok: false,
      result: error(
        'ast.rewrite.path_escape',
        'The rewrite escaped the requested file.'
      ),
    };
  }
  return { ok: true, path: actual };
}

function sortedMatches(
  rawMatches: AstGrepJsonMatch[],
  path: string,
  beforeHash: string
): AstRewriteMatch[] {
  return rawMatches
    .map(raw => ({
      id: matchId(path, beforeHash, raw),
      path,
      byteRange: replacementRange(raw),
      range: raw.range,
      text: raw.text,
      replacement: raw.replacement,
      captures: captures(raw),
    }))
    .sort(
      (left, right) =>
        left.byteRange.start - right.byteRange.start ||
        left.byteRange.end - right.byteRange.end
    );
}

function captures(raw: AstGrepJsonMatch): Record<string, AstRewriteCapture> {
  const result: Record<string, AstRewriteCapture> = {};
  for (const [name, value] of Object.entries(raw.metaVariables?.single ?? {})) {
    result[name] = { kind: 'single', texts: [value.text] };
  }
  for (const [name, values] of Object.entries(raw.metaVariables?.multi ?? {})) {
    result[name] = { kind: 'multi', texts: values.map(value => value.text) };
  }
  for (const [name, value] of Object.entries(
    raw.metaVariables?.transformed ?? {}
  )) {
    result[name] = { kind: 'transformed', texts: [value] };
  }
  return result;
}

function findOverlap(matches: AstRewriteMatch[]): boolean {
  for (let index = 1; index < matches.length; index += 1) {
    const previous = matches[index - 1];
    const current = matches[index];
    if (!previous || !current) continue;
    if (
      current.byteRange.start < previous.byteRange.end ||
      (current.byteRange.start === previous.byteRange.start &&
        current.byteRange.end === previous.byteRange.end)
    ) {
      return true;
    }
  }
  return false;
}

export async function prepareFiles(
  rawMatches: AstGrepJsonMatch[],
  realRoot: string,
  boundary: string,
  cwd: string,
  maxFiles: number,
  maxPatchBytes: number
): Promise<
  | { ok: true; files: PreparedFile[]; matches: AstRewriteMatch[] }
  | { ok: false; result: AstRewriteError }
> {
  const grouped = new Map<string, AstGrepJsonMatch[]>();
  for (const raw of rawMatches) {
    const target = await resolveTarget(raw.file, realRoot, boundary, cwd);
    if (target.ok === false) return { ok: false, result: target.result };
    const items = grouped.get(target.path) ?? [];
    items.push(raw);
    grouped.set(target.path, items);
  }
  if (grouped.size > maxFiles) {
    return {
      ok: false,
      result: error(
        'ast.rewrite.file_limit',
        `The rewrite affects ${grouped.size} files, exceeding maxFiles=${maxFiles}.`,
        { terminalLimit: true }
      ),
    };
  }

  const files: PreparedFile[] = [];
  const allMatches: AstRewriteMatch[] = [];
  let totalPatchBytes = 0;
  for (const absolutePath of [...grouped.keys()].sort()) {
    const before = await readFile(absolutePath);
    if (!validateUtf8(before)) {
      return {
        ok: false,
        result: error(
          'ast.rewrite.encoding_unsupported',
          'Only NUL-free UTF-8 source files can be rewritten.',
          { details: { path: absolutePath } }
        ),
      };
    }
    const beforeHash = hash(before);
    const path = portableRelative(boundary, absolutePath);
    const matches = sortedMatches(
      grouped.get(absolutePath) ?? [],
      path,
      beforeHash
    );
    if (findOverlap(matches)) {
      return {
        ok: false,
        result: error(
          'ast.rewrite.overlap',
          'ast-grep returned overlapping replacement ranges; no changes were prepared.',
          { details: { path } }
        ),
      };
    }
    const after = applyEdits(before, matches);
    if (!after || !validateUtf8(after)) {
      return {
        ok: false,
        result: error(
          'ast.rewrite.source_mismatch',
          'ast-grep match bytes did not agree with the verified source.',
          { details: { path } }
        ),
      };
    }
    const patch = createUnifiedPatch(
      path,
      before.toString('utf8'),
      after.toString('utf8')
    );
    const patchBytes = Buffer.byteLength(patch);
    totalPatchBytes += patchBytes;
    if (totalPatchBytes > maxPatchBytes) {
      return {
        ok: false,
        result: error(
          'ast.rewrite.patch_limit',
          `Unified patches exceed the ${maxPatchBytes}-byte response limit. Narrow the scope.`,
          { terminalLimit: true }
        ),
      };
    }
    const info = await lstat(absolutePath);
    files.push({
      path,
      absolutePath,
      beforeHash,
      afterHash: hash(after),
      matchCount: matches.length,
      patch,
      patchBytes,
      before,
      after,
      mode: info.mode,
      matches,
    });
    allMatches.push(...matches);
  }
  allMatches.sort(
    (left, right) =>
      left.path.localeCompare(right.path) ||
      left.byteRange.start - right.byteRange.start
  );
  return { ok: true, files, matches: allMatches };
}

export function selectPreparedMatches(
  files: PreparedFile[],
  selectedMatchIds: string[],
  maxPatchBytes: number
):
  | { ok: true; files: PreparedFile[]; matches: AstRewriteMatch[] }
  | { ok: false; result: AstRewriteError } {
  const selected = new Set(selectedMatchIds);
  const known = new Set(
    files.flatMap(file => file.matches.map(match => match.id))
  );
  const unknown = selectedMatchIds.filter(id => !known.has(id));
  if (unknown.length > 0) {
    return {
      ok: false,
      result: error(
        'ast.rewrite.selection_invalid',
        'selectedMatchIds contains IDs that are not part of this snapshot.',
        { details: { unknown } }
      ),
    };
  }

  const selectedFiles: PreparedFile[] = [];
  const selectedMatches: AstRewriteMatch[] = [];
  let totalPatchBytes = 0;
  for (const file of files) {
    const matches = file.matches.filter(match => selected.has(match.id));
    if (matches.length === 0) continue;
    const after = applyEdits(file.before, matches);
    if (!after) {
      return {
        ok: false,
        result: error(
          'ast.rewrite.selection_invalid',
          'The selected matches could not be composed safely.'
        ),
      };
    }
    const patch = createUnifiedPatch(
      file.path,
      file.before.toString('utf8'),
      after.toString('utf8')
    );
    const patchBytes = Buffer.byteLength(patch);
    totalPatchBytes += patchBytes;
    if (totalPatchBytes > maxPatchBytes) {
      return {
        ok: false,
        result: error(
          'ast.rewrite.patch_limit',
          `Selected patches exceed the ${maxPatchBytes}-byte response limit. Narrow the selection.`,
          { terminalLimit: true }
        ),
      };
    }
    selectedFiles.push({
      ...file,
      after,
      afterHash: hash(after),
      matchCount: matches.length,
      matches,
      patch,
      patchBytes,
    });
    selectedMatches.push(...matches);
  }
  return { ok: true, files: selectedFiles, matches: selectedMatches };
}
