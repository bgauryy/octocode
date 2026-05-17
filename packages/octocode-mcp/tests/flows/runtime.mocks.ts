import { EventEmitter } from 'events';
import { readFileSync } from 'fs';
import path from 'path';
import { vi } from 'vitest';

type MockExecResult = {
  success: boolean;
  code: number | null;
  stdout: string;
  stderr: string;
};

let fixtureRepoPath = '';

export const mockSafeExec = vi.fn();
export const mockCheckCommandAvailability = vi.fn();
export const mockSpawn = vi.fn();

export function configureLocalResearchFlowRuntime(repoPath: string): void {
  fixtureRepoPath = repoPath;

  mockCheckCommandAvailability.mockReset();
  mockCheckCommandAvailability.mockImplementation(async (command: string) => ({
    available: true,
    command,
    path: `/usr/bin/${command}`,
  }));

  mockSafeExec.mockReset();
  mockSafeExec.mockImplementation(
    async (command: string, args: string[]): Promise<MockExecResult> => {
      if (!/rg$/.test(command)) {
        throw new Error(`Unexpected command: ${command}`);
      }

      if (args.includes('export function computeScore')) {
        return {
          success: true,
          code: 0,
          stdout: buildLocalSearchOutput(),
          stderr: '',
        };
      }

      if (args.includes('\\bcomputeScore\\s*\\(')) {
        return {
          success: true,
          code: 0,
          stdout: buildIncomingCallSearchOutput(),
          stderr: '',
        };
      }

      throw new Error(`Unexpected safeExec args: ${JSON.stringify(args)}`);
    }
  );

  mockSpawn.mockReset();
  mockSpawn.mockImplementation((command: string, args: string[]) => {
    if (!/rg$/.test(command)) {
      throw new Error(`Unexpected spawn command: ${command}`);
    }

    const separatorIndex = args.lastIndexOf('--');
    const symbolName =
      separatorIndex >= 0 && separatorIndex < args.length - 2
        ? args[separatorIndex + 1]
        : undefined;

    if (symbolName !== 'computeScore') {
      throw new Error(`Unexpected spawn args: ${JSON.stringify(args)}`);
    }

    return createMockChildProcess(buildReferenceSearchOutput());
  });
}

export function resetLocalResearchFlowRuntime(): void {
  fixtureRepoPath = '';
  mockSafeExec.mockReset();
  mockCheckCommandAvailability.mockReset();
  mockSpawn.mockReset();
}

function createMockChildProcess(stdout: string): EventEmitter & {
  stdout: EventEmitter;
  stderr: EventEmitter;
  kill: ReturnType<typeof vi.fn>;
} {
  const child = new EventEmitter() as EventEmitter & {
    stdout: EventEmitter;
    stderr: EventEmitter;
    kill: ReturnType<typeof vi.fn>;
  };

  child.stdout = new EventEmitter();
  child.stderr = new EventEmitter();
  child.kill = vi.fn();

  queueMicrotask(() => {
    if (stdout) {
      child.stdout.emit('data', Buffer.from(stdout));
    }
    child.emit('close', 0);
  });

  return child;
}

function buildLocalSearchOutput(): string {
  const scorePath = fixtureFilePath('score.ts');
  const lineText = readFixtureLine('score.ts', 10);

  return [
    rgBegin(scorePath),
    rgMatch(scorePath, 10, lineText, 'computeScore'),
    rgEnd(scorePath),
    rgSummary(1, 1),
  ].join('\n');
}

function buildIncomingCallSearchOutput(): string {
  return [
    rgMatch(
      fixtureFilePath('consumer.ts'),
      4,
      readFixtureLine('consumer.ts', 4),
      'computeScore'
    ),
    rgMatch(
      fixtureFilePath('secondary.ts'),
      7,
      readFixtureLine('secondary.ts', 7),
      'computeScore'
    ),
    rgMatch(
      fixtureFilePath('score.ts'),
      15,
      readFixtureLine('score.ts', 15),
      'computeScore'
    ),
    rgMatch(
      fixtureFilePath('score.ts'),
      10,
      readFixtureLine('score.ts', 10),
      'computeScore'
    ),
  ].join('\n');
}

function buildReferenceSearchOutput(): string {
  return [
    rgMatch(
      fixtureFilePath('consumer.ts'),
      4,
      readFixtureLine('consumer.ts', 4),
      'computeScore'
    ),
    rgMatch(
      fixtureFilePath('secondary.ts'),
      7,
      readFixtureLine('secondary.ts', 7),
      'computeScore'
    ),
    rgMatch(
      fixtureFilePath('score.ts'),
      15,
      readFixtureLine('score.ts', 15),
      'computeScore'
    ),
    rgMatch(
      fixtureFilePath('score.ts'),
      10,
      readFixtureLine('score.ts', 10),
      'computeScore'
    ),
  ].join('\n');
}

function fixtureFilePath(fileName: string): string {
  if (!fixtureRepoPath) {
    throw new Error('Flow runtime mocks were used before fixture setup');
  }

  return path.join(fixtureRepoPath, 'src', fileName);
}

function readFixtureLine(fileName: string, lineNumber: number): string {
  const fileContent = readFileSync(fixtureFilePath(fileName), 'utf8');
  return fileContent.split(/\r?\n/)[lineNumber - 1] ?? '';
}

function rgBegin(filePath: string): string {
  return JSON.stringify({
    type: 'begin',
    data: { path: { text: filePath } },
  });
}

function rgEnd(filePath: string): string {
  return JSON.stringify({
    type: 'end',
    data: {
      path: { text: filePath },
      stats: {
        elapsed: { human: '0.001s' },
        searches: 1,
        searches_with_match: 1,
      },
    },
  });
}

function rgSummary(searches: number, matches: number): string {
  return JSON.stringify({
    type: 'summary',
    data: {
      elapsed_total: { human: '0.001s' },
      stats: {
        elapsed: { human: '0.001s' },
        searches,
        searches_with_match: searches,
        bytes_searched: 512,
        bytes_printed: 256,
        matched_lines: matches,
        matches,
      },
    },
  });
}

function rgMatch(
  filePath: string,
  lineNumber: number,
  lineText: string,
  symbolName: string
): string {
  const start = Math.max(0, lineText.indexOf(symbolName));
  const end = start + symbolName.length;

  return JSON.stringify({
    type: 'match',
    data: {
      path: { text: filePath },
      lines: { text: `${lineText}\n` },
      line_number: lineNumber,
      absolute_offset: lineNumber * 100,
      submatches: [
        {
          match: { text: symbolName },
          start,
          end,
        },
      ],
    },
  });
}
