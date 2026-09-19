import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { afterEach, expect, it } from 'vitest';
import { freshSessionScopedState } from '../src/session-scoped-state.js';
import { createPromptPreflightController } from '../src/tools/prompt-preflight.js';
import {
  captureCurrentContextSources,
  clearCurrentContextSources,
  mergeCurrentContextSources,
} from '../src/tools/context-source-registry.js';
import type { PiContext, PiInstance } from '../src/types.js';

const roots: string[] = [];

afterEach(() => {
  clearCurrentContextSources();
  for (const root of roots.splice(0)) fs.rmSync(root, { recursive: true, force: true });
});

it('registers only an explicitly selected skill as a restore-only current source', () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'prompt-preflight-skill-'));
  roots.push(root);
  const skillPath = path.join(root, 'SKILL.md');
  fs.writeFileSync(skillPath, '# Review\n\nInspect the change.\n');
  const ctx = {
    cwd: root,
    sessionManager: { getSessionId: () => 'prompt-preflight-selected-skill' },
  } as PiContext;
  const session = freshSessionScopedState();
  const controller = createPromptPreflightController({
    pi: {} as PiInstance,
    promptMode: 'octocode-first',
    notify: () => undefined,
    getSession: () => session,
    getFallbackTools: () => [],
    readPhysiology: () => undefined,
  });

  expect(mergeCurrentContextSources(ctx, [])).toEqual([]);
  controller.registerSelectedSkillContext(ctx, {
    name: 'Review',
    description: 'Review a change',
    path: skillPath,
    dir: root,
    source: 'test',
  });

  expect(captureCurrentContextSources(ctx).segments).toEqual([]);
  const current = mergeCurrentContextSources(ctx, []);
  expect(current.map(({ segment }) => segment.id)).toEqual(['selected-skill:review']);
  expect(current[0]?.content).toContain('Inspect the change.');
});
