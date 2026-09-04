import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { afterEach, beforeEach, test } from 'vitest';
import { createSessionArtifactContext } from '../src/tools/session-artifacts.js';
import {
  initializeSessionMemory,
  readSessionMemory,
  renderSessionArtifactPaths,
  SESSION_MEMORY_MAX_BYTES,
  SESSION_MEMORY_RELATIVE_PATH,
  SESSION_MEMORY_TEMPLATE,
} from '../src/tools/session-memory.js';
import {
  appendSessionAuditEntry,
  initializeSessionAudit,
  SESSION_AUDIT_MAX_EVENTS,
  SESSION_AUDIT_RELATIVE_PATH,
} from '../src/tools/session-audit.js';

let priorHome: string | undefined;
let root: string;
let workspace: string;

beforeEach(() => {
  priorHome = process.env['OCTOCODE_HOME'];
  root = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-session-memory-'));
  workspace = path.join(root, 'workspace');
  fs.mkdirSync(workspace);
  process.env['OCTOCODE_HOME'] = path.join(root, 'home');
});

afterEach(() => {
  if (priorHome === undefined) delete process.env['OCTOCODE_HOME'];
  else process.env['OCTOCODE_HOME'] = priorHome;
  fs.rmSync(root, { recursive: true, force: true });
});

function artifactContext() {
  return createSessionArtifactContext({
    cwd: workspace,
    sessionManager: { getSessionId: () => 'session-memory-test' },
  });
}

test('session memory initializes once, preserves agent notes, and registers its producer', () => {
  const artifact = artifactContext();
  const memoryPath = initializeSessionMemory(artifact);
  assert.equal(memoryPath, artifact.resolve(SESSION_MEMORY_RELATIVE_PATH));
  assert.equal(fs.readFileSync(memoryPath, 'utf8'), SESSION_MEMORY_TEMPLATE);
  assert.equal(readSessionMemory(artifact), undefined, 'an untouched template is not projected as memory');

  const learned = SESSION_MEMORY_TEMPLATE.replace(
    '## Decisions\n',
    '## Decisions\n- 2026-09-04 — Keep session artifacts outside the workspace.\n',
  );
  artifact.writeText(SESSION_MEMORY_RELATIVE_PATH, learned);
  initializeSessionMemory(artifact);

  assert.equal(fs.readFileSync(memoryPath, 'utf8'), learned, 'session start never overwrites existing memory');
  assert.equal(readSessionMemory(artifact), learned);
  assert.deepEqual(artifact.inspect()?.producers.memory?.paths, [SESSION_MEMORY_RELATIVE_PATH]);
});

test('session memory projection is byte-bounded and keeps valid UTF-8', () => {
  const artifact = artifactContext();
  initializeSessionMemory(artifact);
  artifact.writeText(
    SESSION_MEMORY_RELATIVE_PATH,
    `${SESSION_MEMORY_TEMPLATE}\n## Handoff\n- ${'🙂'.repeat(SESSION_MEMORY_MAX_BYTES)}\n`,
  );

  const projected = readSessionMemory(artifact);
  assert.ok(projected);
  assert.ok(Buffer.byteLength(projected, 'utf8') <= SESSION_MEMORY_MAX_BYTES);
  assert.equal(projected.includes('\uFFFD'), false, 'truncation must not split a UTF-8 sequence');
});

test('session artifact path context names both files and their ownership', () => {
  const artifact = artifactContext();
  const rendered = renderSessionArtifactPaths({
    memoryPath: artifact.resolve(SESSION_MEMORY_RELATIVE_PATH),
    auditPath: artifact.resolve(SESSION_AUDIT_RELATIVE_PATH),
  });
  assert.match(rendered, /^<session_artifacts>/);
  assert.match(rendered, /memory\.md/);
  assert.match(rendered, /audit\.md/);
  assert.match(rendered, /agent-maintained/);
  assert.match(rendered, /system-written/);
});

test('session audit appends escaped rows, keeps the newest bounded events, and registers its producer', () => {
  const artifact = artifactContext();
  const auditPath = initializeSessionAudit(artifact);
  assert.equal(auditPath, artifact.resolve(SESSION_AUDIT_RELATIVE_PATH));

  for (let index = 0; index < SESSION_AUDIT_MAX_EVENTS + 5; index += 1) {
    appendSessionAuditEntry(artifact, {
      at: new Date(index * 1000).toISOString(),
      event: `plan.step.${index}`,
      detail: index === SESSION_AUDIT_MAX_EVENTS + 4 ? 'line one | line two\ncontinued' : `detail ${index}`,
    });
  }

  const audit = fs.readFileSync(auditPath, 'utf8');
  const eventRows = audit.split('\n').filter((line) => /^\| \d{4}-/.test(line));
  assert.equal(eventRows.length, SESSION_AUDIT_MAX_EVENTS);
  assert.doesNotMatch(audit, /plan\.step\.0\b/);
  assert.match(audit, new RegExp(`plan\\.step\\.${SESSION_AUDIT_MAX_EVENTS + 4}\\b`));
  assert.match(audit, /line one \\| line two continued/);
  assert.match(audit, /system-written/i);
  assert.deepEqual(artifact.inspect()?.producers.audit?.paths, [SESSION_AUDIT_RELATIVE_PATH]);
});
