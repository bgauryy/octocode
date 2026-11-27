// Freeze real production execution at an OS/process boundary; replace no protocol code.
// Each boundary wraps one method of the built runtime (dist/runtime.js) so it calls the original unchanged and then
// stops the whole process with SIGSTOP; the test inspects the database and kills the process.
import { closeSync, fsyncSync, openSync, renameSync, unlinkSync, writeSync } from 'node:fs';
import { createServer } from 'node:net';

function saved(path: string, value: unknown) {
  const temporary = `${path}.${process.pid}.tmp`;
  const descriptor = openSync(temporary, 'w');
  try {
    writeSync(descriptor, JSON.stringify(value));
    fsyncSync(descriptor);
  } finally {
    closeSync(descriptor);
  }
  renameSync(temporary, path);
}

/** The argument list of a method, so a wrapper keeps the signature of the method it wraps. */
type CallOf<Method> = Method extends (...call: infer Call extends unknown[]) => unknown ? Call : never;

const [mode, marker, ...args] = process.argv.slice(2);

if (mode === 'server') {
  const [endpoint, log] = args;
  try { unlinkSync(endpoint); } catch { /* not there */ }
  const server = createServer(client => {
    const chunks: Buffer[] = [];
    client.on('data', chunk => chunks.push(chunk));
    client.on('end', () => {
      const frames = Buffer.concat(chunks).toString('utf8').split('\n').filter(Boolean).map(line => JSON.parse(line));
      const descriptor = openSync(log, 'a');
      try {
        for (const frame of frames) writeSync(descriptor, JSON.stringify({ pid: process.pid, at: Date.now(), frame }) + '\n');
        fsyncSync(descriptor);
      } finally {
        closeSync(descriptor);
      }
      client.end();
    });
    client.on('error', () => undefined);
  });
  server.listen(endpoint, () => saved(marker, { pid: process.pid, listening: true }));
} else {
  const runtime = await import('../../dist/runtime.js');
  const { store: { Store }, ownerLock: { claimDeliveryOwner }, transport: { NativeDelivery }, database: { Database }, cli } = runtime;
  if (mode === 'recovery-owner') {
    const [path, workspace, identity] = args;
    const store = new Store(path, workspace, false, false);
    try {
      const lock = await claimDeliveryOwner(path, identity);
      try {
        store.attach(identity, { transport: 'raw' });
        await store.call(identity, 'heartbeat', { vendorSession: 'new-managed-session' });
        saved(marker, { pid: process.pid, recovered: true });
      } finally {
        await lock.release();
      }
    } catch (error) {
      saved(marker, { pid: process.pid, error: error instanceof Error ? error.message : String(error) });
      process.exitCode = 1;
    } finally {
      store.close();
    }
  } else {
    let frozen = false;
    const freeze = (event: string, details: Record<string, unknown> = {}) => {
      if (frozen) return;
      frozen = true;
      saved(marker, { pid: process.pid, boundary: mode, event, at: Date.now(), ...details });
      process.kill(process.pid, 'SIGSTOP');
    };
    const wrap = <Self extends object, Name extends keyof Self & string>(target: { prototype: Self }, name: Name, around: (this: Self, original: Self[Name], call: CallOf<Self[Name]>) => unknown) => {
      const original = target.prototype[name];
      Reflect.set(target.prototype, name, function wrapped(this: Self, ...call: CallOf<Self[Name]>) { return around.call(this, original, call); });
    };
    if (mode === 'before-submit') wrap(NativeDelivery, 'offer', function (original, call) { freeze('call', { function: 'offer' }); return original.apply(this, call); });
    if (mode === 'uncertain-submit') wrap(NativeDelivery, 'offer', async function (original, call) { const receipt = await original.apply(this, call); freeze('return', { function: 'offer' }); return receipt; });
    if (mode === 'after-submit-before-ack') wrap(Store, 'finishDispatch', function (original, call) {
      const result = original.apply(this, call);
      if ((call[2] ?? null) === null) freeze('return', { function: 'finishDispatch' });
      return result;
    });
    if (mode === 'during-complete') wrap(Database, 'exec', function (original, call) {
      if (call[0] === 'COMMIT') {
        const replies = this.raw.prepare("SELECT id,replyTo FROM messages WHERE key LIKE 'complete:%'").all();
        if (replies.length) {
          const acknowledged = this.raw.prepare('SELECT count(*) AS n FROM deliveries WHERE acknowledgedAt IS NOT NULL').get()?.n;
          freeze('line', { function: 'transaction', inTransaction: this.inTransaction, uncommittedReplies: replies.map(row => [row.id, row.replyTo]), uncommittedAcknowledged: acknowledged });
        }
      }
      return original.apply(this, call);
    });
    process.exitCode = await cli.main(args);
    if (process.exitCode === 0) throw new Error('The requested real crash boundary was not reached');
  }
}
