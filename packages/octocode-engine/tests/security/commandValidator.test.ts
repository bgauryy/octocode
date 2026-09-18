import { afterEach, describe, expect, it } from 'vitest';

import { validateCommand } from '../../src/security/commandValidator.js';
import { securityRegistry } from '../../src/security/registry.js';

const REMOVED_ARCHIVE_BACKEND_COMMANDS = [
  'file',
  'zcat',
  'gunzip',
  'bzcat',
  'xzcat',
  'zstdcat',
  'zstd',
  'lz4cat',
  'brotli',
  'lzfse',
  'tar',
  'unzip',
  'bsdtar',
  '7z',
  '7zz',
] as const;

function valid(command: string, args: string[]): void {
  expect(validateCommand(command, args)).toEqual({ isValid: true });
}

function invalid(command: string, args: string[], message: RegExp): void {
  expect(validateCommand(command, args)).toMatchObject({
    isValid: false,
    error: expect.stringMatching(message),
  });
}

describe('validateCommand', () => {
  afterEach(() => securityRegistry.reset());

  it.each(REMOVED_ARCHIVE_BACKEND_COMMANDS)(
    'rejects removed archive/decompress backend command %s',
    command => {
      invalid(command, ['/tmp/archive'], /not allowed/);
    }
  );

  it('validates argument shape, normalized names, and registered commands', () => {
    expect(validateCommand('rg', null as unknown as string[])).toMatchObject({
      isValid: false,
      error: 'Arguments must be an array',
    });
    valid('/usr/local/bin/rg', ['pattern', '/tmp']);
    invalid('not-a-command', [], /Allowed commands/);

    securityRegistry.addAllowedCommands(['/opt/bin/custom-tool']);
    valid('/usr/bin/custom-tool', ['plain']);
    invalid('/usr/bin/custom-tool', ['$(inject)'], /argument/);
  });

  it('accepts supported ripgrep flags and rejects executable or unknown flags', () => {
    valid('rg', [
      '-Fin',
      '--glob',
      '*.ts',
      '--max-filesize',
      '1M',
      'needle',
      '.',
    ]);
    valid('rg', ['-P', '--', 'value(foo)', '.']);
    valid('rg', ['--', '--pre-glob-safe-as-pattern', '.']);
    invalid('rg', ['--pre', 'cat', 'needle', '.'], /--pre/);
    invalid('rg', ['--pre-glob', '*.gz', 'needle', '.'], /--pre-glob/);
    invalid('rg', ['--engine', 'auto', 'needle', '.'], /--engine/);
    invalid('rg', ['-FiZ', 'needle', '.'], /-FiZ/);
  });

  it('distinguishes search patterns from shell arguments for rg and grep', () => {
    valid('rg', ['value(foo|bar)', '.']);
    valid('rg', ['--glob', '*.(ts|js)', 'needle', '.']);
    valid('grep', ['--include=*.{ts,js}', 'value(foo|bar)', '.']);
    valid('grep', ['--', 'value(foo)', '.']);

    invalid('rg', ['value;rm', '.'], /search pattern/);
    invalid('rg', ['needle', '/tmp/$(touch pwned)'], /argument/);
    invalid('grep', ['`whoami`', '.'], /search pattern/);
    invalid('grep', ['needle', '/tmp/out;rm'], /argument/);
  });

  it('allows bounded find predicates and rejects command-producing operators', () => {
    valid('find', [
      '/tmp',
      '-E',
      '-O3',
      '-maxdepth',
      '3',
      '-type',
      'f',
      '(',
      '-name',
      '*.ts',
      '-o',
      '-iname',
      '*.js',
      ')',
      '-print0',
    ]);
    valid('find', ['/tmp', '--', '-name', '*.ts']);
    invalid('find', ['/tmp', '-exec', 'sh', ';'], /-exec/);
    invalid('find', ['/tmp', '-delete'], /-delete/);
    invalid('find', ['/tmp', '-unknown'], /-unknown/);
    invalid('find', ['/tmp', '-name', '$(touch pwned)'], /search pattern/);
  });

  it('restricts git to safe clone invocations and configuration', () => {
    invalid('git', [], /requires a subcommand/);
    invalid('git', ['-C', '/tmp'], /requires a subcommand/);
    invalid('git', ['status'], /subcommand 'status'/);
    invalid('git', ['-c', 'core.hooksPath=/tmp/hooks', 'clone'], /config key/);

    valid('git', [
      '-C',
      '/tmp',
      '-c',
      'http.version=HTTP/1.1',
      'clone',
      '--depth',
      '1',
      '--single-branch',
      'https://github.com/octocode/repo.git',
      'repo',
    ]);
    valid('git', [
      'clone',
      '-c',
      'http.followRedirects=false',
      '--',
      'ssh://git@github.com/octocode/repo.git',
    ]);
    invalid(
      'git',
      ['clone', '--upload-pack', 'sh', 'https://github.com/x/y'],
      /clone flag/
    );
    invalid(
      'git',
      ['clone', '-c', 'alias.pwn=!sh', 'https://github.com/x/y'],
      /config key/
    );
    invalid('git', ['clone', 'file:///tmp/repo'], /file:\/\//);
    invalid('git', ['clone', 'git://example.com/repo'], /git:\/\//);
    invalid('git', ['clone', 'http://example.com/repo'], /http:\/\//);
  });

  it('restricts sparse-checkout actions and flags', () => {
    invalid('git', ['sparse-checkout'], /requires an action/);
    invalid('git', ['sparse-checkout', 'reapply'], /action 'reapply'/);
    invalid(
      'git',
      ['sparse-checkout', 'set', '--skip-checks', 'src'],
      /sparse-checkout flag/
    );

    for (const action of ['init', 'set', 'add', 'list', 'disable']) {
      valid('git', ['sparse-checkout', action, '--cone', 'src']);
    }
    valid('git', ['sparse-checkout', 'set', '--no-cone', '--', 'src/**']);
  });
});
