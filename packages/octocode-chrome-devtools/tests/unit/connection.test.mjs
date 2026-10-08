import test from 'node:test';
import assert from 'node:assert/strict';
import { WebSocketServer } from 'ws';
import { connectCDP } from '../../dist/engine/cdp-connection.mjs';
import { spawn } from 'node:child_process';

async function fixture(fn) {
  const server = new WebSocketServer({ host: '127.0.0.1', port: 0 });
  await new Promise(resolve => server.once('listening', resolve));
  server.on('connection', socket =>
    socket.on('message', bytes => {
      const call = JSON.parse(String(bytes));
      if (call.method === 'Test.pending') return;
      if (call.method === 'Test.close') {
        socket.close();
        return;
      }
      if (call.method === 'Test.error')
        socket.send(
          JSON.stringify({
            id: call.id,
            error: {
              code: -32000,
              message: 'Rejected',
              data: { reason: 'complete evidence' },
            },
          })
        );
      else {
        socket.send(
          JSON.stringify({
            id: call.id,
            result: { params: call.params, sessionId: call.sessionId },
          })
        );
        socket.send(
          JSON.stringify({
            method: 'Test.event',
            params: { value: 7 },
            sessionId: call.sessionId,
          })
        );
      }
    })
  );
  const session = await connectCDP('ws://127.0.0.1:' + server.address().port, {
    timeoutMs: 1000,
  });
  try {
    await fn(session, 'ws://127.0.0.1:' + server.address().port);
  } finally {
    session.close();
    for (const socket of server.clients) socket.terminate();
    await new Promise(resolve => server.close(resolve));
  }
}
test('real WebSocket requests retain session routing, event metadata and protocol error evidence', () =>
  fixture(async session => {
    const event = new Promise(resolve =>
      session.on('Test.event', (data, meta) => resolve({ data, meta }))
    );
    assert.deepEqual(
      await session.send('Test.echo', { anchor: 'research' }, 'child'),
      { params: { anchor: 'research' }, sessionId: 'child' }
    );
    assert.deepEqual(await event, {
      data: { value: 7 },
      meta: { sessionId: 'child' },
    });
    await assert.rejects(session.send('Test.error'), error => {
      assert.deepEqual(error.protocolError, {
        code: -32000,
        message: 'Rejected',
        data: { reason: 'complete evidence' },
      });
      return true;
    });
  }));
test('pending requests reject on deadline, unexpected close and explicit cleanup', async () => {
  await fixture(async session => {
    await assert.rejects(
      session.send('Test.pending', {}, undefined, { timeoutMs: 30 }),
      /CDP timeout.*Test.pending/
    );
  });
  await fixture(async session => {
    await assert.rejects(
      session.send('Test.close'),
      /WebSocket closed unexpectedly/
    );
    await assert.rejects(session.send('Test.echo'), /Session already closed/);
  });
  await fixture(async session => {
    const request = session.send('Test.pending');
    session.close();
    await assert.rejects(request, /Session closed/);
  });
});
test('event failures from another JavaScript realm keep the handler error evidence', () =>
  fixture(async (_session, url) => {
    const module = new URL(
      '../../dist/engine/cdp-connection.mjs',
      import.meta.url
    ).href;
    const script = `import {connectCDP} from ${JSON.stringify(module)};import {runInNewContext} from 'node:vm';const session=await connectCDP(${JSON.stringify(url)});session.on('Test.event',()=>runInNewContext('Promise.reject(new Error("foreign failure"))'));await session.send('Test.echo');setTimeout(()=>session.close(),30);`;
    const outcome = await new Promise((resolve, reject) => {
      const child = spawn(
        process.execPath,
        ['--input-type=module', '-e', script],
        { stdio: ['ignore', 'ignore', 'pipe'] }
      );
      let stderr = '';
      const deadline = setTimeout(() => {
        child.kill('SIGKILL');
        reject(Error('Foreign handler fixture timed out'));
      }, 5000);
      child.stderr.on('data', bytes => (stderr += bytes));
      child.once('error', error => {
        clearTimeout(deadline);
        reject(error);
      });
      child.once('close', code => {
        clearTimeout(deadline);
        resolve({ code, stderr });
      });
    });
    assert.equal(outcome.code, 1);
    assert.match(
      outcome.stderr,
      /CDP_HANDLER_ERROR.*Test.event.*foreign failure/
    );
  }));
