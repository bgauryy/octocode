/**
 * Optional host embedder for CLI --semantic ranking.
 *
 * Set OCTOCODE_EMBED_CMD to a shell command that reads UTF-8 text on stdin and
 * prints JSON on stdout:
 *   { "embedding": number[], "model"?: string }
 *
 * Example:
 *   OCTOCODE_EMBED_CMD='node ./scripts/embed.mjs'
 *
 * Awareness stays zero-dep: the host owns the model/API. When unset, --semantic
 * stays lexical with an explicit warning.
 */

import { spawnSync } from 'node:child_process';

export interface HostEmbedding {
  embedding: Float32Array;
  model: string;
}

export function resolveEmbedCommand(env: NodeJS.ProcessEnv = process.env): string | null {
  const raw = env['OCTOCODE_EMBED_CMD'];
  if (typeof raw !== 'string') return null;
  const cmd = raw.trim();
  return cmd.length > 0 ? cmd : null;
}

export function runHostEmbedder(
  text: string,
  options: { command?: string | null; timeoutMs?: number; env?: NodeJS.ProcessEnv } = {},
): HostEmbedding {
  const command = options.command ?? resolveEmbedCommand(options.env);
  if (!command) {
    throw new Error('OCTOCODE_EMBED_CMD is not set');
  }
  const timeoutMs = options.timeoutMs ?? 15_000;
  const done = spawnSync(command, {
    input: text,
    encoding: 'utf8',
    shell: true,
    timeout: timeoutMs,
    env: options.env ?? process.env,
    maxBuffer: 8 * 1024 * 1024,
  });
  if (done.error) {
    throw new Error(`OCTOCODE_EMBED_CMD failed to start: ${done.error.message}`);
  }
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
  if (!Array.isArray(values) || values.length === 0 || !values.every(v => typeof v === 'number' && Number.isFinite(v))) {
    throw new Error('OCTOCODE_EMBED_CMD embedding must be a non-empty number[]');
  }
  const modelRaw = record['model'];
  const model = typeof modelRaw === 'string' && modelRaw.trim() ? modelRaw.trim() : 'host-embed';
  return { embedding: Float32Array.from(values as number[]), model };
}
