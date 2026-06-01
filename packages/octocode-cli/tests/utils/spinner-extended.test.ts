import {
  describe,
  it,
  expect,
  vi,
  beforeEach,
  afterEach,
  beforeAll,
  afterAll,
} from 'vitest';

describe('Spinner (extended)', () => {
  let originalWrite: typeof process.stdout.write;
  let writtenOutput: string[];
  let originalMaxListeners: number;
  let exitSpy: ReturnType<typeof vi.spyOn>;
  let errorSpy: ReturnType<typeof vi.spyOn>;

  beforeAll(() => {
    originalMaxListeners = process.getMaxListeners();
    process.setMaxListeners(40);
  });

  afterAll(() => {
    process.setMaxListeners(originalMaxListeners);
  });

  beforeEach(() => {
    vi.resetModules();
    vi.clearAllMocks();
    vi.useFakeTimers();

    writtenOutput = [];
    originalWrite = process.stdout.write;
    process.stdout.write = vi.fn((chunk: string | Uint8Array) => {
      writtenOutput.push(chunk.toString());
      return true;
    }) as typeof process.stdout.write;

    exitSpy = vi
      .spyOn(process, 'exit')
      .mockImplementation(() => undefined as never);
    errorSpy = vi.spyOn(console, 'error').mockImplementation(() => {});
  });

  afterEach(() => {
    vi.useRealTimers();
    process.stdout.write = originalWrite;
    exitSpy.mockRestore();
    errorSpy.mockRestore();
  });

  it('start then clear restores line and cursor', async () => {
    const { Spinner } = await import('../../src/utils/spinner.js');
    const spinner = new Spinner('Clearing');

    spinner.start();
    vi.advanceTimersByTime(80);
    spinner.clear();

    expect(writtenOutput.some(o => o.includes('\x1B[2K'))).toBe(true);
    expect(writtenOutput.some(o => o.includes('\x1B[?25h'))).toBe(true);
  });

  it('start then stop writes symbol and newline', async () => {
    const { Spinner } = await import('../../src/utils/spinner.js');
    const spinner = new Spinner('Done');

    spinner.start();
    spinner.stop();

    expect(writtenOutput.some(o => o.includes('✓'))).toBe(true);
    expect(writtenOutput.some(o => o.includes('\x1B[?25h'))).toBe(true);
  });

  it('start with custom text and indent writes padded output', async () => {
    const { Spinner } = await import('../../src/utils/spinner.js');
    const spinner = new Spinner('');

    spinner.start('Indented', 6);
    vi.advanceTimersByTime(80);

    expect(
      writtenOutput.some(o => o.includes('      ') && o.includes('Indented'))
    ).toBe(true);
    spinner.stop();
  });

  it('update changes text shown on next frame', async () => {
    const { Spinner } = await import('../../src/utils/spinner.js');
    const spinner = new Spinner('A');

    spinner.start();
    spinner.update('B');
    vi.advanceTimersByTime(80);
    expect(writtenOutput.some(o => o.includes('B'))).toBe(true);
    spinner.stop();
  });

  it('succeed, fail, info, and warn use expected symbols', async () => {
    const { Spinner } = await import('../../src/utils/spinner.js');

    const s1 = new Spinner('s');
    s1.start();
    s1.succeed('ok');
    expect(writtenOutput.some(o => o.includes('✓') && o.includes('ok'))).toBe(
      true
    );

    const s2 = new Spinner('f');
    s2.start();
    s2.fail('bad');
    expect(writtenOutput.some(o => o.includes('✗') && o.includes('bad'))).toBe(
      true
    );

    const s3 = new Spinner('i');
    s3.start();
    s3.info('note');
    expect(writtenOutput.some(o => o.includes('ℹ') && o.includes('note'))).toBe(
      true
    );

    const s4 = new Spinner('w');
    s4.start();
    s4.warn('care');
    expect(writtenOutput.some(o => o.includes('⚠') && o.includes('care'))).toBe(
      true
    );
  });

  it('starting again clears previous interval (timer cleared)', async () => {
    const clearIntervalSpy = vi.spyOn(global, 'clearInterval');
    const { Spinner } = await import('../../src/utils/spinner.js');
    const spinner = new Spinner('one');

    spinner.start();
    spinner.start('two');
    expect(clearIntervalSpy).toHaveBeenCalled();
    vi.advanceTimersByTime(80);
    expect(writtenOutput.some(o => o.includes('two'))).toBe(true);
    spinner.stop();
    clearIntervalSpy.mockRestore();
  });

  it('second spinner on same module does not add duplicate process handlers', async () => {
    const { Spinner } = await import('../../src/utils/spinner.js');
    const beforeExit = process.listenerCount('exit');
    const beforeSigint = process.listenerCount('SIGINT');

    const a = new Spinner('a');
    a.start();
    const midExit = process.listenerCount('exit');
    const midSigint = process.listenerCount('SIGINT');

    const b = new Spinner('b');
    b.start();

    expect(process.listenerCount('exit')).toBe(midExit);
    expect(process.listenerCount('SIGINT')).toBe(midSigint);
    expect(midExit).toBeGreaterThanOrEqual(beforeExit + 1);
    expect(midSigint).toBeGreaterThanOrEqual(beforeSigint + 1);

    a.stop();
    b.stop();
  });

  it('SIGINT handler restores cursor and exits', async () => {
    const { Spinner } = await import('../../src/utils/spinner.js');
    new Spinner('sig').start();

    const count = process.listenerCount('SIGINT');
    expect(count).toBeGreaterThan(0);

    process.emit('SIGINT');
    expect(writtenOutput.some(o => o.includes('\x1B[?25h'))).toBe(true);
    expect(exitSpy).toHaveBeenCalledWith(0);
  });

  it('SIGTERM handler restores cursor and exits', async () => {
    const { Spinner } = await import('../../src/utils/spinner.js');
    new Spinner('term').start();

    process.emit('SIGTERM');
    expect(writtenOutput.some(o => o.includes('\x1B[?25h'))).toBe(true);
    expect(exitSpy).toHaveBeenCalledWith(0);
  });

  it('uncaughtException handler restores cursor, logs, and exits 1', async () => {
    const { Spinner } = await import('../../src/utils/spinner.js');
    new Spinner('err').start();

    const err = new Error('kaboom');
    process.emit('uncaughtException', err);

    expect(writtenOutput.some(o => o.includes('\x1B[?25h'))).toBe(true);
    expect(errorSpy).toHaveBeenCalledWith('Uncaught exception:', err);
    expect(exitSpy).toHaveBeenCalledWith(1);
  });
});
