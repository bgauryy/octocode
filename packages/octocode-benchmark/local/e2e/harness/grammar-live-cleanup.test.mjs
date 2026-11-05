import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import {
  awaitProcessExit,
  parseProcessTable,
  processesUnder,
  removeLiveRoot,
  writtenSince,
} from './grammar-live-cleanup.mjs';

const root = '/tmp/octocode-grammar-live-x';
const table = () =>
  `  10     1    00:05 /usr/bin/clangd --compile-commands-dir=${root}/home/m --background-index\n  20    10 01:02:03 /usr/bin/java -jar server.jar\n  30     1    00:01 /usr/bin/other /tmp/elsewhere\n  40     1    00:01 node grammar-live.mjs ${root}\n`;

const refused = () => {
  throw Object.assign(new Error('ENOTEMPTY: directory not empty, rmdir'), {
    code: 'ENOTEMPTY',
  });
};

test('the process table keeps the whole command line', () => {
  assert.deepEqual(parseProcessTable(table())[1], {
    pid: 20,
    ppid: 10,
    elapsed: '01:02:03',
    command: '/usr/bin/java -jar server.jar',
  });
  assert.deepEqual(parseProcessTable('\nnot a row\n'), []);
});

test('a holder names the root or holds a file under it, and the suite is not its own holder', () => {
  const holders = processesUnder(root, {
    table,
    openFilePids: () => '20\n',
    self: 40,
  });
  assert.deepEqual(
    holders.map(row => row.pid),
    [10, 20]
  );
  assert.deepEqual(
    processesUnder(root, { table: () => '', openFilePids: () => '', self: 40 }),
    []
  );
});

test('removal reports success, and a failure names the error, the holders and what changed', () => {
  const tmp = fs.mkdtempSync(
    path.join(os.tmpdir(), 'octocode-live-clean-test-')
  );
  const live = path.join(tmp, 'live');
  fs.mkdirSync(path.join(live, 'deep'), { recursive: true });
  fs.writeFileSync(path.join(live, 'deep/old'), '');
  fs.utimesSync(path.join(live, 'deep/old'), new Date(1000), new Date(1000));
  fs.utimesSync(path.join(live, 'deep'), new Date(1000), new Date(1000));
  const since = Date.now() - 1000;
  fs.writeFileSync(path.join(live, 'deep/new'), '');
  try {
    assert.deepEqual(
      writtenSince(live, since)
        .map(row => row.file)
        .toSorted((a, b) => a.localeCompare(b)),
      ['deep', 'deep/new']
    );
    const failed = removeLiveRoot(live, {
      since,
      remove: refused,
      holders: () => [{ pid: 7 }],
    });
    assert.equal(failed.removed, false);
    assert.match(failed.error, /ENOTEMPTY/u);
    assert.deepEqual(failed.holders, [{ pid: 7 }]);
    assert.ok(failed.writtenSinceStop.some(row => row.file === 'deep/new'));
    assert.deepEqual(removeLiveRoot(live, { since }), { removed: true });
    assert.equal(fs.existsSync(live), false);
  } finally {
    fs.rmSync(tmp, { recursive: true, force: true });
  }
});

test('the wait follows the processes, keeps each first command, and kills nothing', async () => {
  const java = {
    pid: 5,
    ppid: 1,
    elapsed: '00:09',
    command: 'java -data /root',
  };
  const samples = [[java], [{ ...java, command: '(java)' }], []];
  let clock = 0;
  const waited = await awaitProcessExit('/root', {
    holders: () => samples.shift(),
    now: () => clock,
    pause: async ms => {
      clock += ms;
    },
    pollMs: 100,
  });
  assert.equal(waited.waitedMs, 200);
  assert.deepEqual(waited.stillRunning, []);
  assert.deepEqual(waited.processes, [{ ...java, lastSeenMs: 100 }]);
});

test('a process that outlives the wait is reported, not hidden', async () => {
  const stuck = { pid: 6, ppid: 1, elapsed: '09:00', command: 'server /root' };
  let clock = 0;
  const waited = await awaitProcessExit('/root', {
    holders: () => [stuck],
    now: () => clock,
    pause: async ms => {
      clock += ms;
    },
    deadlineMs: 300,
    pollMs: 100,
  });
  assert.equal(waited.waitedMs, 300);
  assert.deepEqual(waited.stillRunning, [stuck]);
});

test('a directory something rebuilt during removal does not count as removed', () => {
  const tmp = fs.mkdtempSync(
    path.join(os.tmpdir(), 'octocode-live-clean-test-')
  );
  try {
    const result = removeLiveRoot(tmp, {
      since: Date.now() - 1000,
      remove: () => {},
      holders: () => [{ pid: 8 }],
    });
    assert.equal(result.removed, false);
    assert.match(result.error, /recreated/u);
    assert.deepEqual(result.holders, [{ pid: 8 }]);
  } finally {
    fs.rmSync(tmp, { recursive: true, force: true });
  }
});
