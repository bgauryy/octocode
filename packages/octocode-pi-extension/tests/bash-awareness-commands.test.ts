import { describe, expect, it } from 'vitest';
import { assertBashCommandAllowed, extractBashWriteTargets } from '../src/tools/bash-tool.js';

const cwd = '/tmp/awareness-shell-fixture';
describe('Awareness CLI shell arguments remain data', () => {
  it.each([
    `node awareness.js message send --subject review --body 'write > /outside-review-fixture/result.txt; cp a /outside-review-fixture/b; tee $LOG'`,
    `node awareness.js message send --subject review --body "write > /outside-review-fixture/result.txt"`,
    `node awareness.js skill install --project-dir "$PWD" --dry-run`,
    `node awareness.js hooks install --project-dir "$PWD" --dry-run`,
    `node awareness.js message send --body 'cp source /outside-review-fixture/result.txt'`,
    `node awareness.js message send --subject 'cp' --body 'just data'`,
  ])('allows a data-only CLI command: %s', command => {
    expect(extractBashWriteTargets(command, cwd)).toEqual([]);
    expect(() => assertBashCommandAllowed(command, cwd)).not.toThrow();
  });
  it.each([
    `node awareness.js message list > /outside-review-fixture/result.txt`,
    `node awareness.js message list | tee /outside-review-fixture/result.txt`,
    `node awareness.js message list; install source /outside-review-fixture/result.txt`,
    `sudo install source /outside-review-fixture/result.txt`,
    `node awareness.js skill install --dry-run && cp source /outside-review-fixture/result.txt`,
    `node awareness.js message send --body "$(printf x > /outside-review-fixture/result.txt)"`,
    `node awareness.js message send --body "🙂 safe" > /outside-review-fixture/result.txt`,
    `install source /outside-review-fixture/result.txt; echo done`,
    `sudo -u root install source /outside-review-fixture/result.txt`,
    `'/bin/cp' source /outside-review-fixture/result.txt`,
    `"cp" source /outside-review-fixture/result.txt`,
    `/bin/cp source /outside-review-fixture/result.txt`,
    `{ cp source /outside-review-fixture/result.txt; }`,
    `if true; then cp source /outside-review-fixture/result.txt; fi`,
    `for item in x; do cp source /outside-review-fixture/result.txt; done`,
    `printf x | '/usr/bin/tee' /outside-review-fixture/result.txt`,
  ])('still blocks real writes: %s', command => {
    expect(() => assertBashCommandAllowed(command, cwd)).toThrow(/blocked/);
  });
});
