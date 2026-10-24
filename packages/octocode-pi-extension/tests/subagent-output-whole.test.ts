import fs from 'node:fs';
import path from 'node:path';
import { describe, expect, it } from 'vitest';
import { shortPath } from '../src/shared/format.js';
import type { ChildNotices, ChildProgress } from '../src/subagents/events.js';
import { runSubagent } from '../src/subagents/process.js';
import { noticeLines } from '../src/subagents/render.js';
import { tmp } from './helpers.js';
import { useScriptedPi } from './subagent-tool-harness.js';


useScriptedPi();

/** Runs a scenario and returns what the parent was told: the last notices update, and the outcome or the error. */
async function run(task: string, scratch: string) {
  const notices: ChildNotices[] = [];
  const onProgress = (update: ChildProgress) => void (update.notices && notices.push(update.notices));
  const id = `a-${task.replace(/\W+/g, '-')}`;
  const result = await runSubagent(task, ['--session-dir', tmp(), '--session-id', id], tmp(), undefined, { id, scratch }, undefined, onProgress).then((outcome) => ({ outcome, error: undefined as Error | undefined }), (error: Error) => ({ outcome: undefined, error }));
  return { ...result, notices: notices.at(-1) };
}

describe('a child\'s notices past the inline page', () => {
  it('are saved whole in notices.txt, each distinct one on a line with its repeat count, and the report names the file', async () => {
    const scratch = tmp();
    const { notices } = await run('wordy notices', scratch);
    const file = path.join(scratch, 'notices.txt');
    expect(notices).toMatchObject({ more: 5, file });
    const lines = fs.readFileSync(file, 'utf8').trimEnd().split('\n');
    expect(lines).toHaveLength(10);
    expect(lines[0]).toBe(`error: long failure ${'detail '.repeat(300)}end`);
    expect(lines[1]).toBe('warning (×9): retrying the cache');
    expect(lines.at(-1)).toBe('warning: distinct warning 8');
    // The inline page is whole notices, and the last line is the way to the rest.
    const text = noticeLines({ notices }).join('\n');
    expect(text).toContain(`- error: long failure ${'detail '.repeat(300)}end`);
    expect(text).toContain(`- +5 more notices; every notice, whole, is in ${shortPath(file)} (read it, or search it)`);
    expect(text).not.toContain('distinct warning 8');
  });

  it('stay inline, with no file, when they fit the page', async () => {
    const scratch = tmp();
    const { notices } = await run('thrash', scratch);
    expect(notices).toMatchObject({ more: 0 });
    expect(notices?.kept).toHaveLength(2);
    expect(notices?.file).toBeUndefined();
    expect(fs.existsSync(path.join(scratch, 'notices.txt'))).toBe(false);
  });

  it('all go inline, whole, when no file can be written: nothing is dropped silently', async () => {
    const { notices } = await run('wordy notices', path.join(tmp(), 'missing', 'folder'));
    expect(notices?.more).toBe(0);
    expect(notices?.file).toBeUndefined();
    expect(notices?.kept).toHaveLength(10);
    expect(notices?.kept.at(-1)).toMatchObject({ text: 'distinct warning 8' });
  });
});

describe('a crashed child\'s stderr', () => {
  it('is shown whole when it is small, with no file', async () => {
    const scratch = tmp();
    const { error } = await run('fail', scratch);
    expect(error?.message).toBe('Subagent exited with code 3: boom');
    expect(fs.existsSync(path.join(scratch, 'stderr.txt'))).toBe(false);
  });

  it('is saved whole when only its tail fits, and the error says how much it shows and where the rest is', async () => {
    const scratch = tmp();
    const { error } = await run('noisy stderr', scratch);
    const whole = Array.from({ length: 300 }, (_, n) => `diagnostic line ${n + 1} of the noisy child`).join('\n') + '\n';
    const file = path.join(scratch, 'stderr.txt');
    expect(fs.readFileSync(file, 'utf8')).toBe(whole);
    expect(error?.message).toContain(`[stderr: last 4000 of ${whole.length} characters; whole: ${shortPath(file)}]`);
    expect(error?.message).toContain('diagnostic line 300 of the noisy child');
    expect(error?.message).not.toContain('diagnostic line 1 of');
  });

  it('says the rest could not be saved when no file can be written', async () => {
    const { error } = await run('noisy stderr', path.join(tmp(), 'missing', 'folder'));
    expect(error?.message).toMatch(/\[stderr: last 4000 of \d+ characters; the rest could not be saved\]/);
  });
});

describe('a crashed child\'s last progress', () => {
  it('is whole, however long it is below the report preview', async () => {
    const { error } = await run('medium narrate then crash', tmp());
    expect(error?.message).toContain(`Last progress: ${'progress detail '.repeat(60)}the end`);
    expect(error?.message).not.toContain('more characters cut');
  });
});
