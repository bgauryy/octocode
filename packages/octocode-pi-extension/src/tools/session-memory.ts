import fs from 'node:fs';
import type { SessionArtifactContext } from './session-artifacts.js';

export const SESSION_MEMORY_RELATIVE_PATH = 'memory.md';
export const SESSION_MEMORY_MAX_BYTES = 4_000;

export const SESSION_MEMORY_TEMPLATE = `# Session memory

Agent-maintained notes for this Pi session. Keep at most 10 one-line entries per section and 200 characters per entry.

## Gotchas

## Improvements

## Findings

## Decisions

## Handoff

## Reflections
`;

export interface SessionArtifactPaths {
  memoryPath: string;
  auditPath: string;
}

export interface SessionMemoryUpdate {
  content: string;
  signature: string;
}

/** Deliver current session memory only when its bounded bytes changed. */
export function projectSessionMemoryUpdate(
  current: string,
  deliveredSignature: string | undefined,
): SessionMemoryUpdate {
  if (current === deliveredSignature) return { content: '', signature: current };
  if (!current) {
    return {
      content: deliveredSignature === undefined ? '' : 'Session memory cleared; no session notes remain.',
      signature: '',
    };
  }
  return { content: current, signature: current };
}

function truncateUtf8(text: string, maxBytes: number): string {
  const bytes = Buffer.from(text, 'utf8');
  if (bytes.length <= maxBytes) return text;
  let end = maxBytes;
  while (end > 0 && (bytes[end]! & 0b1100_0000) === 0b1000_0000) end -= 1;
  return bytes.subarray(0, end).toString('utf8');
}

export function initializeSessionMemory(ctx: SessionArtifactContext): string {
  const memoryPath = ctx.resolve(SESSION_MEMORY_RELATIVE_PATH);
  if (!fs.existsSync(memoryPath)) ctx.writeText(SESSION_MEMORY_RELATIVE_PATH, SESSION_MEMORY_TEMPLATE);
  ctx.registerProducer('memory', SESSION_MEMORY_RELATIVE_PATH);
  return memoryPath;
}

/** Read only meaningful, bounded current bytes for prompt/rehydration projection. */
export function readSessionMemory(ctx: SessionArtifactContext): string | undefined {
  try {
    const memoryPath = ctx.resolve(SESSION_MEMORY_RELATIVE_PATH);
    if (!fs.existsSync(memoryPath)) return undefined;
    const text = fs.readFileSync(memoryPath, 'utf8');
    if (!text.trim() || text.trim() === SESSION_MEMORY_TEMPLATE.trim()) return undefined;
    return truncateUtf8(text, SESSION_MEMORY_MAX_BYTES);
  } catch {
    return undefined;
  }
}

export function renderSessionArtifactPaths(paths: SessionArtifactPaths): string {
  return `<session_artifacts>\nmemory.md (agent-maintained): ${JSON.stringify(paths.memoryPath)}\naudit.md (system-written; never edit): ${JSON.stringify(paths.auditPath)}\n</session_artifacts>`;
}
