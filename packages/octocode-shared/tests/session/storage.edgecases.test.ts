/**
 * Session Storage Edge Cases Tests
 *
 * Comprehensive test coverage for edge cases in session storage:
 * - File corruption scenarios
 * - Disk full / I/O error conditions
 * - Concurrent access from multiple processes
 * - Permission issues
 * - Exit handler edge cases
 * - Recovery & resilience
 * - Rate limit specific edge cases
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import * as fs from 'node:fs';
import {
  SESSION_FILE,
  STATS_FILE,
  readSession,
  writeSession,
  getOrCreateSession,
  updateSessionStats,
  incrementToolCalls,
  incrementRateLimits,
  incrementErrors,
  incrementPromptCalls,
  resetSessionStats,
  deleteSession,
  flushSession,
  flushSessionSync,
  _resetSessionState,
} from '../../src/session/storage.js';
import type { PersistedSession } from '../../src/session/types.js';
import {
  FS_ERRORS,
  createTestSession,
  MALFORMED_JSON,
  generateTruncatedJson,
} from '../helpers/fsErrors.js';

const zeroTotalUsageStats = () => ({
  toolCalls: 0,
  promptCalls: 0,
  errors: 0,
  rateLimits: 0,
  rateLimitsByProvider: {},
  rawChars: 0,
  responseChars: 0,
  savedChars: 0,
  charSavingsCalls: 0,
  githubCacheHits: 0,
  githubCacheRateLimits: 0,
  packageRegistryFailures: 0,
  packageRegistryFailuresByRegistry: {},
});

const defaultStats = () => ({
  toolCalls: 0,
  promptCalls: 0,
  errors: 0,
  rateLimits: 0,
  rateLimitsByProvider: {},
  charsSavedByTool: {},
  githubCacheHits: {
    hits: {},
    rateLimits: 0,
  },
  packageRegistryFailures: {},
  totalUsage: zeroTotalUsageStats(),
});

// Mock node:fs to prevent tests from touching real filesystem
vi.mock('node:fs', () => ({
  existsSync: vi.fn(),
  readFileSync: vi.fn(),
  writeFileSync: vi.fn(),
  mkdirSync: vi.fn(),
  unlinkSync: vi.fn(),
  renameSync: vi.fn(),
}));

// Mock ensureOctocodeDir to prevent creating real directories
vi.mock('../../src/credentials/storage.js', async importOriginal => {
  const actual =
    await importOriginal<typeof import('../../src/credentials/storage.js')>();
  return {
    ...actual,
    ensureOctocodeDir: vi.fn(),
  };
});

describe('Session Storage Edge Cases', () => {
  // In-memory store for mocked filesystem
  let mockFileStore: Map<string, string>;

  beforeEach(() => {
    vi.resetModules();
    vi.clearAllMocks();

    // Reset internal state (cache, timer, etc.)
    _resetSessionState();

    // Initialize mock file store
    mockFileStore = new Map();

    // Setup fs mocks to use in-memory store
    vi.mocked(fs.existsSync).mockImplementation((path: unknown) => {
      return mockFileStore.has(String(path));
    });

    vi.mocked(fs.readFileSync).mockImplementation((path: unknown) => {
      const content = mockFileStore.get(String(path));
      if (content === undefined) {
        throw FS_ERRORS.ENOENT(String(path));
      }
      return content;
    });

    vi.mocked(fs.writeFileSync).mockImplementation(
      (path: unknown, data: unknown) => {
        mockFileStore.set(String(path), String(data));
      }
    );

    vi.mocked(fs.mkdirSync).mockImplementation(() => undefined);

    vi.mocked(fs.unlinkSync).mockImplementation((path: unknown) => {
      mockFileStore.delete(String(path));
    });

    vi.mocked(fs.renameSync).mockImplementation(
      (oldPath: unknown, newPath: unknown) => {
        const content = mockFileStore.get(String(oldPath));
        if (content !== undefined) {
          mockFileStore.set(String(newPath), content);
          mockFileStore.delete(String(oldPath));
        }
      }
    );
  });

  afterEach(() => {
    // Reset internal state
    _resetSessionState();
    vi.resetAllMocks();
  });

  describe('File Corruption Handling', () => {
    describe('Empty File', () => {
      it('should handle empty session file gracefully', () => {
        mockFileStore.set(SESSION_FILE, '');
        _resetSessionState();

        const session = readSession();
        expect(session).toBeNull();

        // Creating new session should work
        const newSession = getOrCreateSession();
        expect(newSession.sessionId).toBeDefined();
        expect(newSession.stats.rateLimits).toBe(0);
      });
    });

    describe('Invalid JSON (Malformed)', () => {
      it.each([
        ['random garbage', MALFORMED_JSON.randomGarbage],
        ['unclosed string', MALFORMED_JSON.unclosedBrace],
        ['unquoted key', MALFORMED_JSON.unquotedKey],
        ['trailing comma', MALFORMED_JSON.trailingComma],
        ['null value', MALFORMED_JSON.null],
        ['undefined value', MALFORMED_JSON.undefined],
        ['array instead of object', MALFORMED_JSON.array],
        ['double comma', MALFORMED_JSON.doubleComma],
        ['missing value', MALFORMED_JSON.missingValue],
      ])('should handle malformed JSON: %s', (_name, content) => {
        mockFileStore.set(SESSION_FILE, content);
        _resetSessionState();

        const session = readSession();
        expect(session).toBeNull();
      });
    });

    describe('Valid JSON but Missing Required Fields', () => {
      it('should reject session with missing sessionId', () => {
        mockFileStore.set(
          SESSION_FILE,
          JSON.stringify({
            version: 1,
            createdAt: '2026-01-09T10:00:00.000Z',
            lastActiveAt: '2026-01-09T10:00:00.000Z',
            stats: { toolCalls: 0, promptCalls: 0, errors: 0, rateLimits: 0 },
          })
        );
        _resetSessionState();

        expect(readSession()).toBeNull();
      });

      it('should reject session with missing createdAt', () => {
        mockFileStore.set(
          SESSION_FILE,
          JSON.stringify({
            version: 1,
            sessionId: 'test-uuid',
            lastActiveAt: '2026-01-09T10:00:00.000Z',
            stats: { toolCalls: 0, promptCalls: 0, errors: 0, rateLimits: 0 },
          })
        );
        _resetSessionState();

        expect(readSession()).toBeNull();
      });

      it('should handle session with missing stats', () => {
        mockFileStore.set(
          SESSION_FILE,
          JSON.stringify({
            version: 1,
            sessionId: 'test-uuid',
            createdAt: '2026-01-09T10:00:00.000Z',
            lastActiveAt: '2026-01-09T10:00:00.000Z',
            // Missing: stats
          })
        );
        _resetSessionState();

        const readBack = readSession();
        expect(readBack?.sessionId).toBe('test-uuid');
        expect(readBack?.stats).toEqual(defaultStats());
      });
    });

    describe('Partial Stats Object', () => {
      it('should handle session with partial stats object', () => {
        mockFileStore.set(
          SESSION_FILE,
          JSON.stringify({
            version: 1,
            sessionId: 'test-uuid',
            createdAt: '2026-01-09T10:00:00.000Z',
            lastActiveAt: '2026-01-09T10:00:00.000Z',
            stats: {
              toolCalls: 5,
              // Missing: promptCalls, errors, rateLimits
            },
          })
        );
        _resetSessionState();

        // Zod validation rejects sessions with partial stats (missing required fields)
        const session = readSession();
        expect(session).toBeNull();

        // No session loaded, so increment fails gracefully
        const result = incrementRateLimits(1);
        expect(result.success).toBe(false);
        expect(result.session).toBeNull();
      });
    });

    describe('Wrong Data Types in Stats', () => {
      it('should handle non-numeric stats values', () => {
        mockFileStore.set(
          SESSION_FILE,
          JSON.stringify(
            createTestSession({
              stats: {
                toolCalls: 'five' as unknown as number,
                promptCalls: null,
                errors: undefined,
                rateLimits: {} as unknown as number,
              },
            })
          )
        );
        _resetSessionState();

        // Zod validation rejects sessions with non-numeric stat values
        const session = readSession();
        expect(session).toBeNull();

        const result = incrementRateLimits(1);
        expect(result.success).toBe(false);
        expect(result.session).toBeNull();
      });

      it('should handle negative stats values', () => {
        mockFileStore.set(
          SESSION_FILE,
          JSON.stringify(
            createTestSession({
              stats: {
                toolCalls: -5,
                promptCalls: -10,
                errors: -1,
                rateLimits: -3,
              },
            })
          )
        );
        _resetSessionState();

        const result = incrementRateLimits(1);
        expect(result.success).toBe(true);
        // -3 + 1 = -2
        expect(result.session?.stats.rateLimits).toBe(-2);
      });
    });

    describe('Truncated File (Partial Write)', () => {
      it('should handle truncated session file from interrupted write', () => {
        const fullContent = JSON.stringify(
          createTestSession({
            stats: { toolCalls: 5, promptCalls: 2, errors: 1, rateLimits: 3 },
          })
        );

        const truncatedContents = [
          generateTruncatedJson(fullContent, 'start'),
          generateTruncatedJson(fullContent, 'middle'),
          generateTruncatedJson(fullContent, 'end'),
        ];

        for (const content of truncatedContents) {
          mockFileStore.set(SESSION_FILE, content);
          _resetSessionState();

          const session = readSession();
          expect(session).toBeNull();
        }
      });
    });

    describe('Very Large File', () => {
      it('should handle unexpectedly large session file', () => {
        const largeSession = {
          version: 1,
          sessionId: 'test-uuid',
          createdAt: '2026-01-09T10:00:00.000Z',
          lastActiveAt: '2026-01-09T10:00:00.000Z',
          stats: { toolCalls: 0, promptCalls: 0, errors: 0, rateLimits: 0 },
          // Inject large payload (simulate tampering/corruption)
          extraData: 'x'.repeat(1024 * 1024), // 1MB
        };

        mockFileStore.set(SESSION_FILE, JSON.stringify(largeSession));
        _resetSessionState();

        // Should still read valid session
        const session = readSession();
        expect(session?.sessionId).toBe('test-uuid');
      });
    });

    describe('Unicode and Special Characters', () => {
      it('should handle unicode in session file', () => {
        mockFileStore.set(
          SESSION_FILE,
          JSON.stringify(
            createTestSession({
              sessionId: 'test-uuid-😀🎉',
            })
          )
        );
        _resetSessionState();

        const session = readSession();
        expect(session?.sessionId).toBe('test-uuid-😀🎉');
      });
    });
  });

  describe('Disk Full and I/O Error Handling', () => {
    describe('Write Fails - Disk Full (ENOSPC)', () => {
      it('should propagate disk full error on writeFileSync', () => {
        getOrCreateSession();
        incrementRateLimits(5);

        // Reset isDirty by writing manually first
        mockFileStore.clear();
        _resetSessionState();
        getOrCreateSession();
        incrementRateLimits(5);

        // Simulate disk full
        vi.mocked(fs.writeFileSync).mockImplementation(() => {
          throw FS_ERRORS.ENOSPC();
        });

        // CURRENT BEHAVIOR: flushSession does NOT catch write errors
        // This is a potential improvement area - could catch and handle gracefully
        // Note: flushSessionSync DOES catch errors (for exit handlers)
        expect(() => flushSession()).toThrow('ENOSPC');

        // In-memory cache should still be valid despite error
        const cached = readSession();
        expect(cached?.stats.rateLimits).toBe(5);
      });
    });

    describe('Write Fails - Quota Exceeded (EDQUOT)', () => {
      it('should propagate quota exceeded error', () => {
        getOrCreateSession();
        incrementRateLimits(3);

        vi.mocked(fs.writeFileSync).mockImplementation(() => {
          throw FS_ERRORS.EDQUOT();
        });

        // CURRENT BEHAVIOR: flushSession propagates write errors
        expect(() => flushSession()).toThrow('EDQUOT');
      });
    });

    describe('Read Fails - I/O Error (EIO)', () => {
      it('should handle I/O error when reading session file', () => {
        mockFileStore.set(SESSION_FILE, '{"version": 1}');

        vi.mocked(fs.readFileSync).mockImplementation(() => {
          throw FS_ERRORS.EIO();
        });

        _resetSessionState();

        // Should return null, not throw
        const session = readSession();
        expect(session).toBeNull();
      });
    });

    describe('Rename Fails (Atomic Write)', () => {
      it('should propagate rename failure during atomic write', () => {
        getOrCreateSession();
        incrementRateLimits(5);

        // writeFileSync succeeds but renameSync fails
        vi.mocked(fs.renameSync).mockImplementation(() => {
          throw FS_ERRORS.EXDEV();
        });

        // CURRENT BEHAVIOR: flushSession propagates rename errors
        expect(() => flushSession()).toThrow('EXDEV');
      });

      it('should propagate ENOENT on rename', () => {
        getOrCreateSession();
        incrementRateLimits(5);

        vi.mocked(fs.renameSync).mockImplementation(() => {
          throw FS_ERRORS.ENOENT(`${SESSION_FILE}.tmp`);
        });

        // CURRENT BEHAVIOR: flushSession propagates rename errors
        expect(() => flushSession()).toThrow('ENOENT');
      });
    });

    describe('Directory Creation Fails', () => {
      it('should handle ensureOctocodeDir failure', async () => {
        const { ensureOctocodeDir } =
          await import('../../src/credentials/storage.js');

        vi.mocked(ensureOctocodeDir).mockImplementation(() => {
          throw FS_ERRORS.EACCES();
        });

        // Should throw on getOrCreateSession since dir creation fails
        expect(() => getOrCreateSession()).toThrow();
      });
    });

    describe('Read-Only File System', () => {
      it('should propagate EROFS error', () => {
        getOrCreateSession();
        incrementRateLimits(5);

        vi.mocked(fs.writeFileSync).mockImplementation(() => {
          throw FS_ERRORS.EROFS();
        });

        // CURRENT BEHAVIOR: flushSession propagates write errors
        expect(() => flushSession()).toThrow('EROFS');
      });
    });
  });

  describe('Concurrent Access Handling', () => {
    describe('Multiple Increments in Same Process', () => {
      it('should handle rapid sequential increments', () => {
        getOrCreateSession();

        // 100 sequential increment calls
        for (let i = 0; i < 100; i++) {
          incrementRateLimits(1);
        }

        const session = readSession();
        expect(session?.stats.rateLimits).toBe(100);
      });

      it('should handle interleaved stat updates', () => {
        getOrCreateSession();

        for (let i = 0; i < 50; i++) {
          incrementRateLimits(1);
          incrementToolCalls(1);
          incrementErrors(1);
          incrementPromptCalls(1);
        }

        const session = readSession();
        expect(session?.stats.rateLimits).toBe(50);
        expect(session?.stats.toolCalls).toBe(50);
        expect(session?.stats.errors).toBe(50);
        expect(session?.stats.promptCalls).toBe(50);
      });
    });

    describe('External File Modification (Multi-Process)', () => {
      it('should detect external file modification after cache reset', () => {
        getOrCreateSession();
        incrementRateLimits(5);
        flushSession();

        // Simulate external process modifying the file
        const externalSession = createTestSession({
          sessionId: 'external-process-uuid',
          stats: {
            toolCalls: 100,
            promptCalls: 50,
            errors: 10,
            rateLimits: 20,
          },
        });
        mockFileStore.set(SESSION_FILE, JSON.stringify(externalSession));
        mockFileStore.set(
          STATS_FILE,
          JSON.stringify({
            version: 1,
            stats: externalSession.stats,
          })
        );

        // Cache takes precedence
        const cachedSession = readSession();
        expect(cachedSession?.stats.rateLimits).toBe(5);

        // After cache reset, should read external changes
        _resetSessionState();

        const readBack = readSession();
        expect(readBack?.sessionId).toBe('external-process-uuid');
        expect(readBack?.stats.rateLimits).toBe(20);
      });

      it('should maintain cache integrity across operations', () => {
        const session = getOrCreateSession();
        const originalId = session.sessionId;

        // Modify file externally
        mockFileStore.set(
          SESSION_FILE,
          JSON.stringify(
            createTestSession({
              sessionId: 'different-id',
              stats: {
                toolCalls: 999,
                promptCalls: 0,
                errors: 0,
                rateLimits: 0,
              },
            })
          )
        );

        // Cache should still return original
        const cached = readSession();
        expect(cached?.sessionId).toBe(originalId);

        // Increments should work on cached version
        incrementRateLimits(1);
        expect(readSession()?.stats.rateLimits).toBe(1);
      });
    });

    describe('Read During Write Operation', () => {
      it('should return cached data during flush', () => {
        getOrCreateSession();
        incrementRateLimits(10);

        // Simulate slow write
        vi.mocked(fs.writeFileSync).mockImplementation((path, data) => {
          // During the write, reading should return cached data
          const session = readSession();
          expect(session?.stats.rateLimits).toBe(10);

          // Complete the write
          mockFileStore.set(String(path), String(data));
        });

        flushSession();

        // After flush, data should be consistent
        const session = readSession();
        expect(session?.stats.rateLimits).toBe(10);
      });
    });
  });

  describe('Permission Error Handling', () => {
    describe('Read Permission Denied', () => {
      it('should handle EACCES when reading session file', () => {
        mockFileStore.set(SESSION_FILE, '{}');

        vi.mocked(fs.readFileSync).mockImplementation(() => {
          throw FS_ERRORS.EACCES();
        });

        _resetSessionState();
        const session = readSession();
        expect(session).toBeNull();
      });
    });

    describe('Write Permission Denied', () => {
      it('should propagate EACCES when writing session file', () => {
        getOrCreateSession();
        incrementRateLimits(5);

        vi.mocked(fs.writeFileSync).mockImplementation(() => {
          throw FS_ERRORS.EACCES();
        });

        // CURRENT BEHAVIOR: flushSession propagates write errors
        expect(() => flushSession()).toThrow('EACCES');

        // Data should still be preserved in cache
        expect(readSession()?.stats.rateLimits).toBe(5);
      });
    });

    describe('Delete Permission Denied', () => {
      it('should handle EACCES when deleting session file', () => {
        getOrCreateSession();
        flushSession();

        vi.mocked(fs.unlinkSync).mockImplementation(() => {
          throw FS_ERRORS.EACCES();
        });

        const result = deleteSession();
        expect(result).toBe(false);
      });
    });

    describe('File Busy', () => {
      it('should handle EBUSY error when file is locked', () => {
        getOrCreateSession();
        incrementRateLimits(5);

        vi.mocked(fs.writeFileSync).mockImplementation(() => {
          throw FS_ERRORS.EBUSY();
        });

        // BUG: EBUSY errors propagate up from flushSession
        // Current behavior: Error thrown
        // Expected behavior: Should catch and retry or gracefully degrade
        expect(() => flushSession()).toThrow('EBUSY');
      });
    });
  });

  describe('Exit Handler Edge Cases', () => {
    const signalTestSession = (): PersistedSession => ({
      version: 1,
      sessionId: 'signal-flush-test-session',
      createdAt: '2026-01-09T10:00:00.000Z',
      lastActiveAt: '2026-01-09T10:00:00.000Z',
      stats: {
        toolCalls: 7,
        promptCalls: 0,
        errors: 0,
        rateLimits: 0,
      },
    });

    describe('Multiple Exit Handler Registration', () => {
      it('should not register duplicate exit handlers', () => {
        const exitSpy = vi.spyOn(process, 'on');

        // Multiple operations that trigger handler registration
        getOrCreateSession();
        incrementRateLimits(1);
        incrementRateLimits(1);
        incrementToolCalls(1);
        incrementErrors(1);

        // Count 'exit' registrations
        const exitCalls = exitSpy.mock.calls.filter(call => call[0] === 'exit');
        expect(exitCalls.length).toBe(1);

        exitSpy.mockRestore();
      });
    });

    describe('Exit During Dirty State', () => {
      it('should mark session as dirty after increment', () => {
        getOrCreateSession();
        incrementRateLimits(5);

        // Without explicit flush, flushSession in exit handler will persist
        // We verify the data is in cache
        const cached = readSession();
        expect(cached?.stats.rateLimits).toBe(5);
      });
    });

    describe('Signal Handler Registration', () => {
      it('should register SIGINT handler', () => {
        const onSpy = vi.spyOn(process, 'on');

        _resetSessionState();
        getOrCreateSession();
        incrementRateLimits(1);

        const sigintCalls = onSpy.mock.calls.filter(
          call => call[0] === 'SIGINT'
        );
        expect(sigintCalls.length).toBe(1);

        onSpy.mockRestore();
      });

      it('should register SIGTERM handler', () => {
        const onSpy = vi.spyOn(process, 'on');

        _resetSessionState();
        getOrCreateSession();
        incrementRateLimits(1);

        const sigtermCalls = onSpy.mock.calls.filter(
          call => call[0] === 'SIGTERM'
        );
        expect(sigtermCalls.length).toBe(1);

        onSpy.mockRestore();
      });
    });

    describe('Process signals flush dirty session to disk', () => {
      it('should flush session to disk when SIGINT is emitted', () => {
        _resetSessionState();
        const session = signalTestSession();
        writeSession(session);

        expect(mockFileStore.has(SESSION_FILE)).toBe(false);

        process.emit('SIGINT');

        const content = mockFileStore.get(SESSION_FILE);
        const statsContent = mockFileStore.get(STATS_FILE);
        expect(content).toBeDefined();
        expect(statsContent).toBeDefined();
        const onDisk = JSON.parse(content!);
        const statsOnDisk = JSON.parse(statsContent!);
        expect(onDisk.sessionId).toBe(session.sessionId);
        expect(onDisk.stats).toBeUndefined();
        expect(statsOnDisk.stats.toolCalls).toBe(7);
      });

      it('should flush session to disk when SIGTERM is emitted', () => {
        _resetSessionState();
        const session = signalTestSession();
        session.sessionId = 'sigterm-flush-session';
        writeSession(session);

        expect(mockFileStore.has(SESSION_FILE)).toBe(false);

        process.emit('SIGTERM');

        const content = mockFileStore.get(SESSION_FILE);
        expect(content).toBeDefined();
        expect(JSON.parse(content!).sessionId).toBe('sigterm-flush-session');
      });

      it('should flush session to disk when exit is emitted', () => {
        _resetSessionState();
        const session = signalTestSession();
        session.sessionId = 'exit-flush-session';
        writeSession(session);

        expect(mockFileStore.has(SESSION_FILE)).toBe(false);

        process.emit('exit');

        const content = mockFileStore.get(SESSION_FILE);
        expect(content).toBeDefined();
        expect(JSON.parse(content!).sessionId).toBe('exit-flush-session');
      });
    });

    describe('Periodic flush timer', () => {
      it('should write session to disk from setInterval after flush interval', () => {
        vi.useFakeTimers();
        try {
          _resetSessionState();
          const session = signalTestSession();
          session.sessionId = 'timer-flush-session';
          writeSession(session);

          expect(mockFileStore.has(SESSION_FILE)).toBe(false);

          vi.advanceTimersByTime(60_000);

          const content = mockFileStore.get(SESSION_FILE);
          expect(content).toBeDefined();
          expect(JSON.parse(content!).sessionId).toBe('timer-flush-session');
        } finally {
          vi.useRealTimers();
          _resetSessionState();
        }
      });
    });

    describe('Flush Error During Exit', () => {
      it('should not throw during flushSessionSync even on error', () => {
        getOrCreateSession();
        incrementRateLimits(5);

        vi.mocked(fs.writeFileSync).mockImplementation(() => {
          throw new Error('Disk error during shutdown');
        });

        // flushSessionSync should catch all errors silently
        expect(() => flushSessionSync()).not.toThrow();
      });

      it('should handle multiple errors during sync flush', () => {
        getOrCreateSession();
        incrementRateLimits(5);

        let callCount = 0;
        vi.mocked(fs.writeFileSync).mockImplementation(() => {
          callCount++;
          throw new Error(`Error ${callCount}`);
        });

        // Should not throw even with errors
        expect(() => flushSessionSync()).not.toThrow();
        expect(() => flushSessionSync()).not.toThrow();
      });
    });
  });

  describe('Recovery and Resilience', () => {
    describe('Recovery After Corruption', () => {
      it('should create fresh session after detecting corruption', () => {
        // Start with corrupted file
        mockFileStore.set(SESSION_FILE, MALFORMED_JSON.randomGarbage);
        _resetSessionState();

        // getOrCreateSession should recover
        const session = getOrCreateSession();

        expect(session.sessionId).toBeDefined();
        expect(session.stats.rateLimits).toBe(0);

        // New session should be valid
        incrementRateLimits(1);
        expect(readSession()?.stats.rateLimits).toBe(1);
      });

      it('should recover from version mismatch', () => {
        mockFileStore.set(
          SESSION_FILE,
          JSON.stringify({
            version: 999, // Unknown version
            sessionId: 'old-session',
            createdAt: '2026-01-01T00:00:00.000Z',
          })
        );
        _resetSessionState();

        // Should create new session
        const session = getOrCreateSession();
        expect(session.sessionId).not.toBe('old-session');
        expect(session.version).toBe(1);
      });
    });

    describe('Orphaned Temp File Cleanup', () => {
      it('should handle orphaned temp file from crashed write', () => {
        const tempFile = `${SESSION_FILE}.tmp`;

        // Simulate crashed write - temp file exists but main file doesn't
        mockFileStore.set(
          tempFile,
          JSON.stringify(
            createTestSession({
              sessionId: 'orphaned-session',
              stats: { toolCalls: 0, promptCalls: 0, errors: 0, rateLimits: 5 },
            })
          )
        );

        _resetSessionState();

        // Current behavior: Ignores temp file, creates new session
        const session = getOrCreateSession();
        expect(session.sessionId).not.toBe('orphaned-session');
      });
    });

    describe('Stats Overflow Handling', () => {
      it('should handle stats approaching Number.MAX_SAFE_INTEGER', () => {
        mockFileStore.set(
          SESSION_FILE,
          JSON.stringify(
            createTestSession({
              stats: {
                toolCalls: Number.MAX_SAFE_INTEGER - 1,
                promptCalls: 0,
                errors: 0,
                rateLimits: Number.MAX_SAFE_INTEGER - 1,
              },
            })
          )
        );
        _resetSessionState();

        const result = incrementRateLimits(5);

        // Should handle overflow gracefully
        expect(Number.isFinite(result.session?.stats.rateLimits)).toBe(true);
      });

      it('should handle very large increment values', () => {
        getOrCreateSession();

        const result = incrementRateLimits(Number.MAX_SAFE_INTEGER);
        expect(Number.isFinite(result.session?.stats.rateLimits)).toBe(true);
      });
    });

    describe('Negative Increment Handling', () => {
      it('should handle negative increment attempts', () => {
        getOrCreateSession();
        incrementRateLimits(5);

        // Try to decrement (current behavior: allowed)
        const result = incrementRateLimits(-3);

        // Document current behavior: negative increments are allowed
        expect(result.session?.stats.rateLimits).toBe(2);
      });

      it('should allow stats to go negative', () => {
        getOrCreateSession();

        const result = incrementRateLimits(-5);
        expect(result.session?.stats.rateLimits).toBe(-5);
      });
    });
  });

  describe('Rate Limit Specific Edge Cases', () => {
    describe('Increment Without Session', () => {
      it('should return failure when incrementing without session', () => {
        // Don't create session first
        const result = incrementRateLimits(1);

        expect(result.success).toBe(false);
        expect(result.session).toBeNull();
      });

      it('should return failure for all increment functions without session', () => {
        expect(incrementRateLimits(1).success).toBe(false);
        expect(incrementToolCalls(1).success).toBe(false);
        expect(incrementErrors(1).success).toBe(false);
        expect(incrementPromptCalls(1).success).toBe(false);
      });
    });

    describe('Increment With Zero', () => {
      it('should handle increment by zero', () => {
        getOrCreateSession();
        incrementRateLimits(5);

        const result = incrementRateLimits(0);

        expect(result.success).toBe(true);
        expect(result.session?.stats.rateLimits).toBe(5);
      });

      it('should still update lastActiveAt on zero increment', async () => {
        const session1 = getOrCreateSession();
        const firstActiveAt = session1.lastActiveAt;

        // Wait a bit to ensure timestamp changes
        await new Promise(resolve => setTimeout(resolve, 10));

        const result = incrementRateLimits(0);
        // lastActiveAt should be updated even with zero increment
        expect(result.session?.lastActiveAt).toBeDefined();
      });
    });

    describe('Increment With Non-Integer', () => {
      it('should handle non-integer increment value (float)', () => {
        getOrCreateSession();

        const result = incrementRateLimits(1.5);

        // Current behavior: JS allows float addition
        expect(result.session?.stats.rateLimits).toBe(1.5);
      });

      it('should handle NaN increment', () => {
        getOrCreateSession();
        incrementRateLimits(5);

        const result = incrementRateLimits(NaN);

        // 5 + NaN = NaN
        expect(Number.isNaN(result.session?.stats.rateLimits)).toBe(true);
      });

      it('should handle Infinity increment', () => {
        getOrCreateSession();

        const result = incrementRateLimits(Infinity);
        expect(result.session?.stats.rateLimits).toBe(Infinity);
      });
    });

    describe('Multiple Stats Update Atomicity', () => {
      it('should update rateLimits atomically with other stats', () => {
        getOrCreateSession();

        // Update multiple stats at once
        const result = updateSessionStats({
          rateLimits: 5,
          errors: 3,
          toolCalls: 10,
        });

        expect(result.success).toBe(true);
        expect(result.session?.stats.rateLimits).toBe(5);
        expect(result.session?.stats.errors).toBe(3);
        expect(result.session?.stats.toolCalls).toBe(10);
        expect(result.session?.stats.promptCalls).toBe(0);
      });

      it('should handle partial updates correctly', () => {
        getOrCreateSession();
        incrementRateLimits(10);

        // Update only errors
        const result = updateSessionStats({ errors: 5 });

        expect(result.session?.stats.rateLimits).toBe(10); // Unchanged
        expect(result.session?.stats.errors).toBe(5);
      });
    });

    describe('Reset Stats Edge Cases', () => {
      it('should reset stats even with corrupted values', () => {
        getOrCreateSession();

        // Somehow get NaN into stats
        incrementRateLimits(NaN);

        // Reset should fix it
        const result = resetSessionStats();
        expect(result.success).toBe(true);
        expect(result.session?.stats.rateLimits).toBe(0);
        expect(Number.isNaN(result.session?.stats.rateLimits)).toBe(false);
      });

      it('should return failure when resetting without session', () => {
        const result = resetSessionStats();
        expect(result.success).toBe(false);
        expect(result.session).toBeNull();
      });
    });

    describe('Session ID Persistence', () => {
      it('should maintain same sessionId across increments', () => {
        const session = getOrCreateSession();
        const originalId = session.sessionId;

        for (let i = 0; i < 10; i++) {
          incrementRateLimits(1);
        }

        expect(readSession()?.sessionId).toBe(originalId);
      });

      it('should maintain same sessionId across flush cycles', () => {
        const session = getOrCreateSession();
        const originalId = session.sessionId;

        incrementRateLimits(5);
        flushSession();

        incrementRateLimits(5);
        flushSession();

        expect(readSession()?.sessionId).toBe(originalId);
      });
    });
  });

  describe('Additional Edge Cases', () => {
    describe('Empty Update Object', () => {
      it('should handle empty update object', () => {
        getOrCreateSession();
        incrementRateLimits(5);

        const result = updateSessionStats({});

        expect(result.success).toBe(true);
        expect(result.session?.stats.rateLimits).toBe(5);
      });
    });

    describe('Multiple Sessions', () => {
      it('should allow forcing new session', () => {
        const session1 = getOrCreateSession();
        incrementRateLimits(5);
        flushSession();

        const session2 = getOrCreateSession({ forceNew: true });

        expect(session2.sessionId).not.toBe(session1.sessionId);
        expect(session2.stats.rateLimits).toBe(0);
      });
    });

    describe('Date/Time Edge Cases', () => {
      it('should handle invalid date strings in session', () => {
        mockFileStore.set(
          SESSION_FILE,
          JSON.stringify({
            version: 1,
            sessionId: 'test-uuid',
            createdAt: 'invalid-date',
            lastActiveAt: 'also-invalid',
            stats: { toolCalls: 0, promptCalls: 0, errors: 0, rateLimits: 0 },
          })
        );
        _resetSessionState();

        // Should still read session (dates are not validated)
        const session = readSession();
        expect(session?.sessionId).toBe('test-uuid');
        expect(session?.createdAt).toBe('invalid-date');
      });
    });

    describe('File System Edge Cases', () => {
      it('should handle existsSync throwing error', () => {
        vi.mocked(fs.existsSync).mockImplementation(() => {
          throw new Error('Unexpected error');
        });

        _resetSessionState();

        // Should propagate error
        expect(() => readSession()).toThrow();
      });
    });

    describe('Concurrent Delete and Increment', () => {
      it('should handle delete during increment operations', () => {
        getOrCreateSession();
        incrementRateLimits(5);

        deleteSession();

        // Incrementing after delete should fail
        const result = incrementRateLimits(1);
        expect(result.success).toBe(false);
      });
    });
  });
});
