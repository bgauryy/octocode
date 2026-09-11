import fs from 'node:fs';
import type { SessionArtifactContext } from './session-artifacts.js';

export const SESSION_MEMORY_RELATIVE_PATH = 'memory.md';
export const SESSION_MEMORY_MAX_BYTES = 4_000;

export const SESSION_MEMORY_TEMPLATE = `# Session memory

Keep only facts needed to resume after context loss. A pending decision or evidence pointer belongs here; routine progress and raw logs do not. Repetition crowds out unfinished work. Update after a meaningful event, with at most 10 one-line entries total and 200 characters per entry; keep the whole file within 4000 UTF-8 bytes. Preserve the next action first and leave unused sections empty.

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

export type SessionMemoryIssueCode =
  | 'file-too-large'
  | 'too-many-entries'
  | 'entry-too-long';

export interface SessionMemoryIssue {
  code: SessionMemoryIssueCode;
  message: string;
}

export type SessionMemoryReadState =
  | { state: 'empty'; content?: undefined }
  | { state: 'ready'; content: string }
  | { state: 'invalid'; content?: undefined; issues: SessionMemoryIssue[] }
  | { state: 'unavailable'; content?: undefined; message: string };

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

function validateSessionMemory(text: string): SessionMemoryIssue[] {
  const issues: SessionMemoryIssue[] = [];
  const bytes = Buffer.byteLength(text, 'utf8');
  if (bytes > SESSION_MEMORY_MAX_BYTES) {
    issues.push({
      code: 'file-too-large',
      message: `memory.md is ${bytes} bytes; maximum ${SESSION_MEMORY_MAX_BYTES}`,
    });
  }
  const lines = text.split(/\r?\n/);
  const firstSection = lines.findIndex(line => /^##\s+/.test(line));
  const entries = (firstSection < 0 ? [] : lines.slice(firstSection + 1)).filter(
    line => line.trim() && !/^##\s+/.test(line)
  );
  if (entries.length > 10) {
    issues.push({
      code: 'too-many-entries',
      message: `memory.md has ${entries.length} entries; maximum 10`,
    });
  }
  entries.forEach((entry, index) => {
    const length = Array.from(entry).length;
    if (length > 200) {
      issues.push({
        code: 'entry-too-long',
        message: `memory.md entry ${index + 1} has ${length} characters; maximum 200`,
      });
    }
  });
  return issues;
}

export function initializeSessionMemory(ctx: SessionArtifactContext): string {
  const memoryPath = ctx.resolve(SESSION_MEMORY_RELATIVE_PATH);
  if (!fs.existsSync(memoryPath)) ctx.writeText(SESSION_MEMORY_RELATIVE_PATH, SESSION_MEMORY_TEMPLATE);
  ctx.registerProducer('memory', SESSION_MEMORY_RELATIVE_PATH);
  return memoryPath;
}

/** Read meaningful memory with explicit empty, invalid, and unavailable states. */
export function readSessionMemoryState(ctx: SessionArtifactContext): SessionMemoryReadState {
  try {
    const memoryPath = ctx.resolve(SESSION_MEMORY_RELATIVE_PATH);
    if (!fs.existsSync(memoryPath)) return { state: 'empty' };
    const text = fs.readFileSync(memoryPath, 'utf8');
    if (!text.trim() || text.trim() === SESSION_MEMORY_TEMPLATE.trim())
      return { state: 'empty' };
    const issues = validateSessionMemory(text);
    if (issues.length > 0) return { state: 'invalid', issues };
    return { state: 'ready', content: text };
  } catch (error) {
    return {
      state: 'unavailable',
      message: error instanceof Error ? error.message : String(error),
    };
  }
}

/** Compatibility projection for context sources that only accept text. */
export function readSessionMemory(ctx: SessionArtifactContext): string | undefined {
  return readSessionMemoryState(ctx).content;
}

export function renderSessionArtifactPaths(paths: SessionArtifactPaths): string {
  return `<session_artifacts>\nmemory.md (agent-maintained): ${JSON.stringify(paths.memoryPath)}\naudit.md (system-written; never edit): ${JSON.stringify(paths.auditPath)}\n</session_artifacts>`;
}
