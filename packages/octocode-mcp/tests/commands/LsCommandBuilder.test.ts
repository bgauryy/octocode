import { describe, it, expect, afterEach } from 'vitest';
import { LsCommandBuilder } from '../../../octocode-tools-core/src/commands/LsCommandBuilder.js';

describe('LsCommandBuilder', () => {
  const originalPlatform = process.platform;

  afterEach(() => {
    Object.defineProperty(process, 'platform', { value: originalPlatform });
  });

  describe('basic command building', () => {
    it('should create an ls command', () => {
      const builder = new LsCommandBuilder();
      const { command } = builder.build();

      expect(command).toBe('ls');
    });

    it('should build a simple ls command', () => {
      const builder = new LsCommandBuilder();
      const { command, args } = builder.simple('/path').build();

      expect(command).toBe('ls');
      expect(args).toContain('/path');
    });

    it('should insert -- before path in simple mode', () => {
      const builder = new LsCommandBuilder();
      const { args } = builder.simple('--sort=time').build();

      const separatorIndex = args.indexOf('--');
      expect(separatorIndex).toBeGreaterThan(-1);
      expect(args[separatorIndex + 1]).toBe('--sort=time');
    });
  });

  describe('fromQuery', () => {
    it('should add --color=never by default', () => {
      const builder = new LsCommandBuilder();
      const { args } = builder.fromQuery({ path: '/test' }).build();

      expect(args).toContain('--color=never');
    });

    it('should add quoting style on Linux', () => {
      Object.defineProperty(process, 'platform', { value: 'linux' });

      const builder = new LsCommandBuilder();
      const { args } = builder.fromQuery({ path: '/test' }).build();

      expect(args).toContain('--quoting-style=literal');
    });

    it('should not add quoting style on macOS', () => {
      Object.defineProperty(process, 'platform', { value: 'darwin' });

      const builder = new LsCommandBuilder();
      const { args } = builder.fromQuery({ path: '/test' }).build();

      expect(args).not.toContain('--quoting-style=literal');
    });

    it('should handle details flag', () => {
      const builder = new LsCommandBuilder();
      const { args } = builder
        .fromQuery({ path: '/test', details: true })
        .build();

      expect(args).toContain('-l');
    });

    it('should add time-style on Linux with details', () => {
      Object.defineProperty(process, 'platform', { value: 'linux' });

      const builder = new LsCommandBuilder();
      const { args } = builder
        .fromQuery({ path: '/test', details: true })
        .build();

      expect(args).toContain('--time-style=long-iso');
    });

    it('should handle hidden flag', () => {
      const builder = new LsCommandBuilder();
      const { args } = builder
        .fromQuery({ path: '/test', hidden: true })
        .build();

      expect(args).toContain('-A');
    });

    it('fromQuery does not add -h (humanReadable removed from schema)', () => {
      const builder = new LsCommandBuilder();
      const { args } = builder.fromQuery({ path: '/test' }).build();

      expect(args).not.toContain('-h');
    });

    it('should handle recursive flag', () => {
      const builder = new LsCommandBuilder();
      const { args } = builder
        .fromQuery({ path: '/test', recursive: true })
        .build();

      expect(args).toContain('-R');
    });

    it('should handle reverse flag', () => {
      const builder = new LsCommandBuilder();
      const { args } = builder
        .fromQuery({ path: '/test', reverse: true })
        .build();

      expect(args).toContain('-r');
    });

    it('should handle sortBy size', () => {
      const builder = new LsCommandBuilder();
      const { args } = builder
        .fromQuery({ path: '/test', sortBy: 'size' })
        .build();

      expect(args).toContain('-S');
    });

    it('should handle sortBy time', () => {
      const builder = new LsCommandBuilder();
      const { args } = builder
        .fromQuery({ path: '/test', sortBy: 'time' })
        .build();

      expect(args).toContain('-t');
    });

    it('should handle sortBy extension', () => {
      const builder = new LsCommandBuilder();
      const { args } = builder
        .fromQuery({ path: '/test', sortBy: 'extension' })
        .build();

      expect(args).toContain('-X');
    });

    it('should handle sortBy name (default)', () => {
      const builder = new LsCommandBuilder();
      const { args } = builder
        .fromQuery({ path: '/test', sortBy: 'name' })
        .build();

      expect(args).not.toContain('-S');
      expect(args).not.toContain('-t');
      expect(args).not.toContain('-X');
    });

    it('should add group-directories-first on Linux with name sort', () => {
      Object.defineProperty(process, 'platform', { value: 'linux' });

      const builder = new LsCommandBuilder();
      const { args } = builder.fromQuery({ path: '/test' }).build();

      expect(args).toContain('--group-directories-first');
    });

    it('should add group-directories-first on Linux when sortBy is name', () => {
      Object.defineProperty(process, 'platform', { value: 'linux' });

      const builder = new LsCommandBuilder();
      const { args } = builder
        .fromQuery({ path: '/test', sortBy: 'name' })
        .build();

      expect(args).toContain('--group-directories-first');
    });

    it('should not add group-directories-first when sortBy is not name', () => {
      Object.defineProperty(process, 'platform', { value: 'linux' });

      const builder = new LsCommandBuilder();
      const { args } = builder
        .fromQuery({ path: '/test', sortBy: 'time' })
        .build();

      expect(args).not.toContain('--group-directories-first');
    });

    it('should add -1 for single column when not details', () => {
      const builder = new LsCommandBuilder();
      const { args } = builder.fromQuery({ path: '/test' }).build();

      expect(args).toContain('-1');
    });

    it('should not add -1 when details is true', () => {
      const builder = new LsCommandBuilder();
      const { args } = builder
        .fromQuery({ path: '/test', details: true })
        .build();

      expect(args).not.toContain('-1');
    });

    it('should include path at the end', () => {
      const builder = new LsCommandBuilder();
      const { args } = builder.fromQuery({ path: '/test/dir' }).build();

      expect(args[args.length - 1]).toBe('/test/dir');
    });

    it('should insert -- before path in fromQuery mode', () => {
      const builder = new LsCommandBuilder();
      const { args } = builder.fromQuery({ path: '--sort=time' }).build();

      const separatorIndex = args.indexOf('--');
      expect(separatorIndex).toBeGreaterThan(-1);
      expect(args[separatorIndex + 1]).toBe('--sort=time');
    });
  });

  describe('chainable methods', () => {
    it('should chain detailed method', () => {
      const builder = new LsCommandBuilder();
      const { args } = builder.detailed().build();

      expect(args).toContain('-l');
    });

    it('should chain all method', () => {
      const builder = new LsCommandBuilder();
      const { args } = builder.all().build();

      expect(args).toContain('-A');
    });

    it('should chain humanReadable method', () => {
      const builder = new LsCommandBuilder();
      const { args } = builder.humanReadable().build();

      expect(args).toContain('-h');
    });

    it('should chain recursive method', () => {
      const builder = new LsCommandBuilder();
      const { args } = builder.recursive().build();

      expect(args).toContain('-R');
    });

    it('should chain sortBySize method', () => {
      const builder = new LsCommandBuilder();
      const { args } = builder.sortBySize().build();

      expect(args).toContain('-S');
    });

    it('should chain sortByTime method', () => {
      const builder = new LsCommandBuilder();
      const { args } = builder.sortByTime().build();

      expect(args).toContain('-t');
    });

    it('should chain reverse method', () => {
      const builder = new LsCommandBuilder();
      const { args } = builder.reverse().build();

      expect(args).toContain('-r');
    });

    it('should chain path method', () => {
      const builder = new LsCommandBuilder();
      const { args } = builder.path('/custom/path').build();

      expect(args).toContain('/custom/path');
    });

    it('should chain multiple methods', () => {
      const builder = new LsCommandBuilder();
      const { args } = builder
        .detailed()
        .all()
        .humanReadable()
        .sortByTime()
        .reverse()
        .path('/test')
        .build();

      expect(args).toContain('-l');
      expect(args).toContain('-A');
      expect(args).toContain('-h');
      expect(args).toContain('-t');
      expect(args).toContain('-r');
      expect(args).toContain('/test');
    });
  });

  describe('complex queries', () => {
    it('should build complete query with all options', () => {
      Object.defineProperty(process, 'platform', { value: 'linux' });

      const builder = new LsCommandBuilder();
      const { command, args } = builder
        .fromQuery({
          path: '/home/user/project',
          details: true,
          hidden: true,
          recursive: true,
          reverse: true,
          sortBy: 'time',
        })
        .build();

      expect(command).toBe('ls');
      expect(args).toContain('--color=never');
      expect(args).toContain('--quoting-style=literal');
      expect(args).toContain('-l');
      expect(args).toContain('--time-style=long-iso');
      expect(args).toContain('-A');
      expect(args).toContain('-R');
      expect(args).toContain('-r');
      expect(args).toContain('-t');
      expect(args).toContain('/home/user/project');
    });
  });

  describe('reset', () => {
    it('should reset builder state', () => {
      const builder = new LsCommandBuilder();
      builder.detailed().all().path('/test');

      const { args: argsBefore } = builder.build();
      expect(argsBefore.length).toBeGreaterThan(0);

      builder.reset();
      const { args: argsAfter } = builder.build();
      expect(argsAfter.length).toBe(0);
    });
  });
});
