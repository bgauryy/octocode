import { describe, it, expect, vi, beforeEach } from 'vitest';
import {
  OctocodeLogger,
  createLogger,
  LoggerFactory,
} from '../../src/utils/core/logger.js';
import type { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';

describe('Logger', () => {
  let mockServer: Partial<McpServer>;

  beforeEach(() => {
    mockServer = {
      isConnected: vi.fn().mockReturnValue(true),
      sendLoggingMessage: vi.fn().mockResolvedValue(undefined),
    };
  });

  describe('OctocodeLogger', () => {
    it('should log info messages', async () => {
      const logger = new OctocodeLogger(mockServer as McpServer, 'test');

      await logger.info('Test message', { key: 'value' });

      expect(mockServer.sendLoggingMessage).toHaveBeenCalledWith({
        level: 'info',
        logger: expect.stringContaining('Octocode'),
        data: expect.objectContaining({
          message: 'Test message',
          key: 'value',
          timestamp: expect.any(String),
        }),
      });
    });

    it('should log warning messages', async () => {
      const logger = new OctocodeLogger(mockServer as McpServer, 'test');

      await logger.warning('Warning message');

      expect(mockServer.sendLoggingMessage).toHaveBeenCalledWith({
        level: 'warning',
        logger: expect.stringContaining('Octocode'),
        data: expect.objectContaining({
          message: 'Warning message',
          timestamp: expect.any(String),
        }),
      });
    });

    it('should log error messages', async () => {
      const logger = new OctocodeLogger(mockServer as McpServer, 'test');

      await logger.error('Error message', { error: 'details' });

      expect(mockServer.sendLoggingMessage).toHaveBeenCalledWith({
        level: 'error',
        logger: expect.stringContaining('Octocode'),
        data: expect.objectContaining({
          message: 'Error message',
          error: 'details',
          timestamp: expect.any(String),
        }),
      });
    });

    it('should log debug messages', async () => {
      const logger = new OctocodeLogger(mockServer as McpServer, 'test');

      await logger.debug('Debug message');

      expect(mockServer.sendLoggingMessage).toHaveBeenCalledWith({
        level: 'debug',
        logger: expect.stringContaining('Octocode'),
        data: expect.objectContaining({
          message: 'Debug message',
          timestamp: expect.any(String),
        }),
      });
    });

    it('should include component in logger prefix', async () => {
      const logger = new OctocodeLogger(mockServer as McpServer, 'myComponent');

      await logger.info('Test');

      expect(mockServer.sendLoggingMessage).toHaveBeenCalledWith(
        expect.objectContaining({
          logger: expect.stringContaining('myComponent'),
        })
      );
    });

    it('should use default component name if not provided', async () => {
      const logger = new OctocodeLogger(mockServer as McpServer);

      await logger.info('Test');

      expect(mockServer.sendLoggingMessage).toHaveBeenCalledWith(
        expect.objectContaining({
          logger: expect.stringContaining('core'),
        })
      );
    });

    it('should handle server not connected gracefully', async () => {
      mockServer.isConnected = vi.fn().mockReturnValue(false);
      const logger = new OctocodeLogger(mockServer as McpServer, 'test');

      await expect(logger.info('Test')).resolves.not.toThrow();

      expect(mockServer.sendLoggingMessage).not.toHaveBeenCalled();
    });

    it('should handle sendLoggingMessage errors gracefully', async () => {
      mockServer.sendLoggingMessage = vi
        .fn()
        .mockRejectedValue(new Error('Send failed'));
      const logger = new OctocodeLogger(mockServer as McpServer, 'test');

      await expect(logger.info('Test')).resolves.not.toThrow();
    });

    it('should include timestamp in ISO format', async () => {
      const logger = new OctocodeLogger(mockServer as McpServer, 'test');

      await logger.info('Test');

      expect(mockServer.sendLoggingMessage).toHaveBeenCalledWith(
        expect.objectContaining({
          data: expect.objectContaining({
            timestamp: expect.stringMatching(
              /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z$/
            ),
          }),
        })
      );
    });

    it('should handle logging without additional data', async () => {
      const logger = new OctocodeLogger(mockServer as McpServer, 'test');

      await logger.info('Simple message');

      expect(mockServer.sendLoggingMessage).toHaveBeenCalledWith({
        level: 'info',
        logger: expect.any(String),
        data: {
          message: 'Simple message',
          timestamp: expect.any(String),
        },
      });
    });

    it('should merge additional data with message and timestamp', async () => {
      const logger = new OctocodeLogger(mockServer as McpServer, 'test');

      await logger.info('Test', { custom: 'value', count: 42 });

      expect(mockServer.sendLoggingMessage).toHaveBeenCalledWith({
        level: 'info',
        logger: expect.any(String),
        data: {
          message: 'Test',
          timestamp: expect.any(String),
          custom: 'value',
          count: 42,
        },
      });
    });

    it('should redact path-like fields from log payloads', async () => {
      const logger = new OctocodeLogger(mockServer as McpServer, 'test');

      await logger.info('Path log', {
        cwd: '/Users/dev/project',
        nested: { filePath: 'src/security/pathValidator.ts' },
      });

      expect(mockServer.sendLoggingMessage).toHaveBeenCalledWith({
        level: 'info',
        logger: expect.any(String),
        data: expect.objectContaining({
          message: 'Path log',
          cwd: '[REDACTED_LOCAL_PATH]',
          nested: { filePath: '[REDACTED_LOCAL_PATH]' },
        }),
      });
    });

    it('should redact path-like text embedded in message', async () => {
      const logger = new OctocodeLogger(mockServer as McpServer, 'test');

      await logger.error('Cannot open /Users/dev/private/file.txt');

      expect(mockServer.sendLoggingMessage).toHaveBeenCalledWith(
        expect.objectContaining({
          data: expect.objectContaining({
            message: expect.not.stringContaining('/Users/dev/private/file.txt'),
          }),
        })
      );
    });

    it('should handle all log levels', async () => {
      const logger = new OctocodeLogger(mockServer as McpServer, 'test');

      await logger.info('Info');
      await logger.warning('Warning');
      await logger.error('Error');
      await logger.debug('Debug');

      expect(mockServer.sendLoggingMessage).toHaveBeenCalledTimes(4);
      expect(mockServer.sendLoggingMessage).toHaveBeenNthCalledWith(
        1,
        expect.objectContaining({ level: 'info' })
      );
      expect(mockServer.sendLoggingMessage).toHaveBeenNthCalledWith(
        2,
        expect.objectContaining({ level: 'warning' })
      );
      expect(mockServer.sendLoggingMessage).toHaveBeenNthCalledWith(
        3,
        expect.objectContaining({ level: 'error' })
      );
      expect(mockServer.sendLoggingMessage).toHaveBeenNthCalledWith(
        4,
        expect.objectContaining({ level: 'debug' })
      );
    });
  });

  describe('createLogger', () => {
    it('should create logger with component name', () => {
      const logger = createLogger(mockServer as McpServer, 'testComponent');

      expect(logger).toBeInstanceOf(OctocodeLogger);
    });

    it('should create logger without component name', () => {
      const logger = createLogger(mockServer as McpServer);

      expect(logger).toBeInstanceOf(OctocodeLogger);
    });

    it('should create functional logger', async () => {
      const logger = createLogger(mockServer as McpServer, 'factory');

      await logger.info('Test from factory');

      expect(mockServer.sendLoggingMessage).toHaveBeenCalledWith(
        expect.objectContaining({
          logger: expect.stringContaining('factory'),
        })
      );
    });
  });

  describe('LoggerFactory', () => {
    beforeEach(() => {
      (LoggerFactory as any).loggers.clear();
    });

    it('should create and cache logger instances', () => {
      const logger1 = LoggerFactory.getLogger(
        mockServer as McpServer,
        'component1'
      );
      const logger2 = LoggerFactory.getLogger(
        mockServer as McpServer,
        'component1'
      );

      expect(logger1).toBe(logger2);
    });

    it('should create different loggers for different components', () => {
      const logger1 = LoggerFactory.getLogger(
        mockServer as McpServer,
        'component1'
      );
      const logger2 = LoggerFactory.getLogger(
        mockServer as McpServer,
        'component2'
      );

      expect(logger1).not.toBe(logger2);
    });

    it('should return OctocodeLogger instances', () => {
      const logger = LoggerFactory.getLogger(mockServer as any, 'component');

      expect(logger).toBeInstanceOf(OctocodeLogger);
    });

    it('should create functional loggers from factory', async () => {
      const logger = LoggerFactory.getLogger(
        mockServer as McpServer,
        'factoryTest'
      );

      await logger.info('Factory test message');

      expect(mockServer.sendLoggingMessage).toHaveBeenCalledWith(
        expect.objectContaining({
          logger: expect.stringContaining('factoryTest'),
        })
      );
    });

    it('should cache multiple different loggers', () => {
      const logger1 = LoggerFactory.getLogger(mockServer as McpServer, 'comp1');
      const logger2 = LoggerFactory.getLogger(mockServer as McpServer, 'comp2');
      const logger3 = LoggerFactory.getLogger(mockServer as McpServer, 'comp3');

      const logger1Again = LoggerFactory.getLogger(
        mockServer as McpServer,
        'comp1'
      );

      expect(logger1).toBe(logger1Again);
      expect(logger1).not.toBe(logger2);
      expect(logger2).not.toBe(logger3);
    });
  });

  describe('Error Recovery', () => {
    it('should not throw when logging fails due to disconnected server', async () => {
      mockServer.isConnected = vi.fn().mockReturnValue(false);
      const logger = new OctocodeLogger(mockServer as McpServer, 'test');

      await expect(
        Promise.all([
          logger.info('Test 1'),
          logger.warning('Test 2'),
          logger.error('Test 3'),
          logger.debug('Test 4'),
        ])
      ).resolves.not.toThrow();
    });

    it('should not throw when sendLoggingMessage throws', async () => {
      mockServer.sendLoggingMessage = vi.fn().mockImplementation(() => {
        throw new Error('Logging failed');
      });
      const logger = new OctocodeLogger(mockServer as McpServer, 'test');

      await expect(logger.info('Test')).resolves.not.toThrow();
    });

    it('should not throw when sendLoggingMessage rejects', async () => {
      mockServer.sendLoggingMessage = vi
        .fn()
        .mockRejectedValue(new Error('Network error'));
      const logger = new OctocodeLogger(mockServer as McpServer, 'test');

      await expect(logger.error('Test')).resolves.not.toThrow();
    });
  });

  describe('Version in Logger Prefix', () => {
    it('should include version number in logger prefix', async () => {
      const logger = new OctocodeLogger(mockServer as McpServer, 'test');

      await logger.info('Test');

      const sendLoggingMessage = mockServer.sendLoggingMessage;
      expect(sendLoggingMessage).toBeDefined();
      const mockCalls = vi.mocked(sendLoggingMessage!).mock.calls;
      expect(mockCalls.length).toBeGreaterThan(0);
      const callArgs = mockCalls[0]?.[0];
      expect(callArgs?.logger).toMatch(/Octocode-\d+\.\d+\.\d+:test/);
    });
  });
});
