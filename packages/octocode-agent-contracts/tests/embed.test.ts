import { describe, expect, it } from 'vitest';
import {
  bytesToEmbedding,
  cosineSimilarity,
  embeddingToBytes,
  isEmbeddingEnabled,
  resolveEmbedCommand,
  runHostEmbedder,
} from '../src/embed.js';

describe('resolveEmbedCommand / isEmbeddingEnabled', () => {
  it('trims and treats blank/unset as null', () => {
    expect(resolveEmbedCommand({ OCTOCODE_EMBED_CMD: '  node x.mjs  ' })).toBe('node x.mjs');
    expect(resolveEmbedCommand({ OCTOCODE_EMBED_CMD: '   ' })).toBeNull();
    expect(resolveEmbedCommand({})).toBeNull();
    expect(isEmbeddingEnabled({ OCTOCODE_EMBED_CMD: 'x' })).toBe(true);
    expect(isEmbeddingEnabled({})).toBe(false);
  });
});

describe('cosineSimilarity', () => {
  it('is 1 for identical, 0 for orthogonal, 0 for mismatched/empty', () => {
    const a = Float32Array.from([1, 0, 0]);
    expect(cosineSimilarity(a, Float32Array.from([1, 0, 0]))).toBeCloseTo(1, 6);
    expect(cosineSimilarity(a, Float32Array.from([0, 1, 0]))).toBeCloseTo(0, 6);
    expect(cosineSimilarity(a, Float32Array.from([1, 0]))).toBe(0);
    expect(cosineSimilarity(new Float32Array(0), new Float32Array(0))).toBe(0);
    expect(cosineSimilarity(a, Float32Array.from([0, 0, 0]))).toBe(0);
  });
});

describe('embedding BLOB round-trip', () => {
  it('serializes and decodes stably, even from an unaligned offset', () => {
    const original = Float32Array.from([0.5, -1.25, 3.0, 42]);
    const bytes = embeddingToBytes(original);
    expect(Array.from(bytesToEmbedding(bytes))).toEqual(Array.from(original));

    // SQLite may hand back a Buffer at a non-4-byte-aligned offset.
    const padded = new Uint8Array(bytes.byteLength + 1);
    padded.set(bytes, 1);
    const unaligned = padded.subarray(1);
    expect(Array.from(bytesToEmbedding(unaligned))).toEqual(Array.from(original));
  });
});

describe('runHostEmbedder', () => {
  it('runs a host command and parses the JSON contract', () => {
    const command = `${process.execPath} -e "let s='';process.stdin.on('data',d=>s+=d).on('end',()=>process.stdout.write(JSON.stringify({embedding:[1,2,3],model:'m'})))"`;
    const result = runHostEmbedder('hello', { command });
    expect(Array.from(result.embedding)).toEqual([1, 2, 3]);
    expect(result.model).toBe('m');
  });

  it('throws when unset and on a non-zero exit', () => {
    expect(() => runHostEmbedder('hi', { command: '' })).toThrow('OCTOCODE_EMBED_CMD is not set');
    const bad = `${process.execPath} -e "process.exit(7)"`;
    expect(() => runHostEmbedder('hi', { command: bad })).toThrow('exited 7');
  });

  it('rejects non-JSON and malformed embeddings', () => {
    const notJson = `${process.execPath} -e "process.stdout.write('nope')"`;
    expect(() => runHostEmbedder('hi', { command: notJson })).toThrow('not JSON');
    const empty = `${process.execPath} -e "process.stdout.write(JSON.stringify({embedding:[]}))"`;
    expect(() => runHostEmbedder('hi', { command: empty })).toThrow('non-empty number[]');
  });
});
