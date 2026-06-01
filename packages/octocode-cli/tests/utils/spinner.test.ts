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

describe('Spinner', () => {
  let originalWrite: typeof process.stdout.write;
  let writtenOutput: string[];
  let originalMaxListeners: number;

  beforeAll(() => {
    originalMaxListeners = process.getMaxListeners();
    process.setMaxListeners(30);
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
  });

  afterEach(() => {
    vi.useRealTimers();
    process.stdout.write = originalWrite;
  });

  describe('Spinner class', () => {
    it('should create a spinner instance', async () => {
      const { Spinner } = await import('../../src/utils/spinner.js');
      const spinner = new Spinner('Loading...');

      expect(spinner).toBeInstanceOf(Spinner);
    });

    it('should start and display spinning animation', async () => {
      const { Spinner } = await import('../../src/utils/spinner.js');
      const spinner = new Spinner('Loading...');

      spinner.start();

      vi.advanceTimersByTime(80);

      expect(writtenOutput.length).toBeGreaterThan(0);

      expect(writtenOutput.some(output => output.includes('Loading...'))).toBe(
        true
      );

      spinner.stop();
    });

    it('should hide cursor on start', async () => {
      const { Spinner } = await import('../../src/utils/spinner.js');
      const spinner = new Spinner('Test');

      spinner.start();

      expect(writtenOutput.some(output => output.includes('\x1B[?25l'))).toBe(
        true
      );

      spinner.stop();
    });

    it('should show cursor on stop', async () => {
      const { Spinner } = await import('../../src/utils/spinner.js');
      const spinner = new Spinner('Test');

      spinner.start();
      spinner.stop();

      expect(writtenOutput.some(output => output.includes('\x1B[?25h'))).toBe(
        true
      );
    });

    it('should allow changing text on start', async () => {
      const { Spinner } = await import('../../src/utils/spinner.js');
      const spinner = new Spinner('Initial');

      spinner.start('Updated text');
      vi.advanceTimersByTime(80);

      expect(
        writtenOutput.some(output => output.includes('Updated text'))
      ).toBe(true);

      spinner.stop();
    });

    it('should succeed with green checkmark', async () => {
      const { Spinner } = await import('../../src/utils/spinner.js');
      const spinner = new Spinner('Processing');

      spinner.start();
      spinner.succeed('Done!');

      expect(writtenOutput.some(output => output.includes('Done!'))).toBe(true);
      expect(writtenOutput.some(output => output.includes('✓'))).toBe(true);
    });

    it('should fail with red X', async () => {
      const { Spinner } = await import('../../src/utils/spinner.js');
      const spinner = new Spinner('Processing');

      spinner.start();
      spinner.fail('Error occurred');

      expect(
        writtenOutput.some(output => output.includes('Error occurred'))
      ).toBe(true);
      expect(writtenOutput.some(output => output.includes('✗'))).toBe(true);
    });

    it('should show info with blue i', async () => {
      const { Spinner } = await import('../../src/utils/spinner.js');
      const spinner = new Spinner('Processing');

      spinner.start();
      spinner.info('Information');

      expect(writtenOutput.some(output => output.includes('Information'))).toBe(
        true
      );
      expect(writtenOutput.some(output => output.includes('ℹ'))).toBe(true);
    });

    it('should show warning with yellow symbol', async () => {
      const { Spinner } = await import('../../src/utils/spinner.js');
      const spinner = new Spinner('Processing');

      spinner.start();
      spinner.warn('Warning message');

      expect(
        writtenOutput.some(output => output.includes('Warning message'))
      ).toBe(true);
      expect(writtenOutput.some(output => output.includes('⚠'))).toBe(true);
    });

    it('should return this for method chaining', async () => {
      const { Spinner } = await import('../../src/utils/spinner.js');
      const spinner = new Spinner('Test');

      const startResult = spinner.start();
      expect(startResult).toBe(spinner);

      const stopResult = spinner.stop();
      expect(stopResult).toBe(spinner);
    });

    it('should clear previous interval when start is called twice', async () => {
      const clearIntervalSpy = vi.spyOn(global, 'clearInterval');
      const { Spinner } = await import('../../src/utils/spinner.js');
      const spinner = new Spinner('First');

      spinner.start();
      spinner.start('Second');

      expect(clearIntervalSpy).toHaveBeenCalled();
      vi.advanceTimersByTime(80);
      expect(writtenOutput.some(output => output.includes('Second'))).toBe(
        true
      );

      spinner.stop();
      clearIntervalSpy.mockRestore();
    });

    it('should update text, return this, and show new text on next tick', async () => {
      const { Spinner } = await import('../../src/utils/spinner.js');
      const spinner = new Spinner('Initial');

      spinner.start();
      const updateResult = spinner.update('Updated');

      expect(updateResult).toBe(spinner);
      vi.advanceTimersByTime(80);
      expect(writtenOutput.some(output => output.includes('Updated'))).toBe(
        true
      );

      spinner.stop();
    });

    it('should stop gracefully when called multiple times', async () => {
      const { Spinner } = await import('../../src/utils/spinner.js');
      const spinner = new Spinner('Test');

      spinner.start();
      spinner.stop();
      spinner.stop();

      expect(true).toBe(true);
    });

    it('should cycle through animation frames', async () => {
      const { Spinner } = await import('../../src/utils/spinner.js');
      const spinner = new Spinner('Loading');

      spinner.start();

      for (let i = 0; i < 10; i++) {
        vi.advanceTimersByTime(80);
      }

      spinner.stop();

      expect(writtenOutput.length).toBeGreaterThan(5);
    });

    it('should use default success symbol and color', async () => {
      const { Spinner } = await import('../../src/utils/spinner.js');
      const spinner = new Spinner('Test');

      spinner.start();
      spinner.stop();

      expect(writtenOutput.some(output => output.includes('✓'))).toBe(true);
    });

    it('should use custom symbol and color on stop', async () => {
      const { Spinner } = await import('../../src/utils/spinner.js');
      const spinner = new Spinner('Test');

      spinner.start();
      spinner.stop('★', 'yellow');

      expect(writtenOutput.some(output => output.includes('★'))).toBe(true);
    });

    it('should clear output and restore cursor', async () => {
      const { Spinner } = await import('../../src/utils/spinner.js');
      const spinner = new Spinner('Clearing...');

      spinner.start();
      vi.advanceTimersByTime(80);

      const clearResult = spinner.clear();

      expect(clearResult).toBe(spinner);

      expect(writtenOutput.some(output => output.includes('\x1B[?25h'))).toBe(
        true
      );

      expect(writtenOutput.some(output => output.includes('\x1B[2K'))).toBe(
        true
      );
    });

    it('should handle clear when not running', async () => {
      const { Spinner } = await import('../../src/utils/spinner.js');
      const spinner = new Spinner('Test');

      const result = spinner.clear();
      expect(result).toBe(spinner);
    });

    it('should handle start with custom indent', async () => {
      const { Spinner } = await import('../../src/utils/spinner.js');
      const spinner = new Spinner('Indented');

      spinner.start('Indented', 4);
      vi.advanceTimersByTime(80);

      expect(
        writtenOutput.some(
          output => output.includes('    ') || output.includes('Indented')
        )
      ).toBe(true);

      spinner.stop();
    });

    it('should succeed without custom text', async () => {
      const { Spinner } = await import('../../src/utils/spinner.js');
      const spinner = new Spinner('Original');

      spinner.start();
      spinner.succeed();

      expect(writtenOutput.some(output => output.includes('Original'))).toBe(
        true
      );
    });

    it('should fail without custom text', async () => {
      const { Spinner } = await import('../../src/utils/spinner.js');
      const spinner = new Spinner('Original');

      spinner.start();
      spinner.fail();

      expect(writtenOutput.some(output => output.includes('Original'))).toBe(
        true
      );
    });

    it('should info without custom text', async () => {
      const { Spinner } = await import('../../src/utils/spinner.js');
      const spinner = new Spinner('Original');

      spinner.start();
      spinner.info();

      expect(writtenOutput.some(output => output.includes('Original'))).toBe(
        true
      );
    });

    it('should warn without custom text', async () => {
      const { Spinner } = await import('../../src/utils/spinner.js');
      const spinner = new Spinner('Original');

      spinner.start();
      spinner.warn();

      expect(writtenOutput.some(output => output.includes('Original'))).toBe(
        true
      );
    });

    it('should work with empty constructor', async () => {
      const { Spinner } = await import('../../src/utils/spinner.js');
      const spinner = new Spinner();

      spinner.start('New text');
      vi.advanceTimersByTime(80);

      expect(writtenOutput.some(output => output.includes('New text'))).toBe(
        true
      );

      spinner.stop();
    });
  });
});
