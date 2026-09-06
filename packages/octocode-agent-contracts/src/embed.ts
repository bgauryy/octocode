/**
 * embed.ts — optional host-owned semantic-embedding primitives, shared by the
 * Awareness store.
 *
 * The model/API is owned by the host, not this package: set `OCTOCODE_EMBED_CMD`
 * to a shell command that reads UTF-8 text on stdin and prints JSON on stdout:
 *   { "embedding": number[], "model"?: string }
 * When unset, callers fall back to lexical recall — no configuration required.
 *
 * Pure and dependency-free (only `node:child_process`), so both zero-dependency
 * packages can consume it without pulling in anything else.
 */
import { spawnSync } from 'node:child_process';

export interface HostEmbedding {
  embedding: Float32Array;
  model: string;
}

/** Resolve the host embed command from env, or null when unset/blank. */
export function resolveEmbedCommand(env: NodeJS.ProcessEnv = process.env): string | null {
  const raw = env['OCTOCODE_EMBED_CMD'];
  if (typeof raw !== 'string') return null;
  const cmd = raw.trim();
  return cmd.length > 0 ? cmd : null;
}

/** True when a host embedder is configured. */
export function isEmbeddingEnabled(env: NodeJS.ProcessEnv = process.env): boolean {
  return resolveEmbedCommand(env) !== null;
}

/**
 * Run the host embedder over `text`. Throws on any failure (unset command,
 * non-zero exit, malformed JSON) so callers can decide to fall back to lexical.
 */
export function runHostEmbedder(
  text: string,
  options: { command?: string | null; timeoutMs?: number; env?: NodeJS.ProcessEnv } = {},
): HostEmbedding {
  const command = options.command ?? resolveEmbedCommand(options.env);
  if (!command) throw new Error('OCTOCODE_EMBED_CMD is not set');
  const done = spawnSync(command, {
    input: text,
    encoding: 'utf8',
    shell: true,
    timeout: options.timeoutMs ?? 15_000,
    env: options.env ?? process.env,
    maxBuffer: 8 * 1024 * 1024,
  });
  if (done.error) throw new Error(`OCTOCODE_EMBED_CMD failed to start: ${done.error.message}`);
  if (done.status !== 0) {
    const err = (done.stderr || done.stdout || '').trim().slice(0, 400);
    throw new Error(`OCTOCODE_EMBED_CMD exited ${done.status}${err ? `: ${err}` : ''}`);
  }
  const stdout = (done.stdout || '').trim();
  if (!stdout) throw new Error('OCTOCODE_EMBED_CMD returned empty stdout');
  let parsed: unknown;
  try {
    parsed = JSON.parse(stdout);
  } catch {
    throw new Error('OCTOCODE_EMBED_CMD stdout is not JSON');
  }
  if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) {
    throw new Error('OCTOCODE_EMBED_CMD JSON must be an object with embedding[]');
  }
  const record = parsed as Record<string, unknown>;
  const values = record['embedding'];
  if (!Array.isArray(values) || values.length === 0 || !values.every((v) => typeof v === 'number' && Number.isFinite(v))) {
    throw new Error('OCTOCODE_EMBED_CMD embedding must be a non-empty number[]');
  }
  const modelRaw = record['model'];
  const model = typeof modelRaw === 'string' && modelRaw.trim() ? modelRaw.trim() : 'host-embed';
  return { embedding: Float32Array.from(values as number[]), model };
}

/** Cosine similarity of two equal-length vectors; 0 when either is zero/mismatched. */
export function cosineSimilarity(a: Float32Array, b: Float32Array): number {
  if (a.length === 0 || a.length !== b.length) return 0;
  let dot = 0, normA = 0, normB = 0;
  for (let i = 0; i < a.length; i++) {
    dot += a[i]! * b[i]!;
    normA += a[i]! * a[i]!;
    normB += b[i]! * b[i]!;
  }
  const denom = Math.sqrt(normA) * Math.sqrt(normB);
  return denom === 0 ? 0 : dot / denom;
}

/** Serialize a Float32Array to raw little-endian bytes for BLOB storage. */
export function embeddingToBytes(embedding: Float32Array): Uint8Array {
  return new Uint8Array(embedding.buffer, embedding.byteOffset, embedding.byteLength);
}

/**
 * Decode BLOB bytes back to a Float32Array. Copies into an aligned buffer so a
 * non-4-byte-aligned byteOffset (possible for SQLite-returned buffers) is safe.
 */
export function bytesToEmbedding(bytes: Uint8Array): Float32Array {
  const copy = new Uint8Array(bytes.byteLength);
  copy.set(bytes);
  return new Float32Array(copy.buffer, 0, Math.floor(copy.byteLength / 4));
}
