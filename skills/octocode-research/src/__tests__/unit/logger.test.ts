import { describe, it, expect } from 'vitest';
import { sanitizeQueryParams } from '../../utils/logger.js';

describe('sanitizeQueryParams', () => {
  it('redacts token parameters', () => {
    const query = { token: 'secret123', path: '/src' };
    const result = sanitizeQueryParams(query);

    expect(result.token).toBe('[REDACTED]');
    expect(result.path).toBe('/src');
  });

  it('redacts password parameters', () => {
    const query = { password: 'mypassword', username: 'admin' };
    const result = sanitizeQueryParams(query);

    expect(result.password).toBe('[REDACTED]');
    expect(result.username).toBe('admin');
  });

  it('redacts key parameters', () => {
    const query = { api_key: 'key123', query: 'search' };
    const result = sanitizeQueryParams(query);

    expect(result.api_key).toBe('[REDACTED]');
    expect(result.query).toBe('search');
  });

  it('redacts secret parameters', () => {
    const query = { clientSecret: 'secret', clientId: 'id123' };
    const result = sanitizeQueryParams(query);

    expect(result.clientSecret).toBe('[REDACTED]');
    expect(result.clientId).toBe('id123');
  });

  it('redacts auth parameters', () => {
    const query = { authorization: 'Bearer token', page: '1' };
    const result = sanitizeQueryParams(query);

    expect(result.authorization).toBe('[REDACTED]');
    expect(result.page).toBe('1');
  });

  it('redacts credential parameters', () => {
    const query = { credentials: 'user:pass', format: 'json' };
    const result = sanitizeQueryParams(query);

    expect(result.credentials).toBe('[REDACTED]');
    expect(result.format).toBe('json');
  });

  it('handles case-insensitive matching', () => {
    const query = {
      TOKEN: 'secret',
      ApiKey: 'key',
      PASSWORD: 'pass',
    };
    const result = sanitizeQueryParams(query);

    expect(result.TOKEN).toBe('[REDACTED]');
    expect(result.ApiKey).toBe('[REDACTED]');
    expect(result.PASSWORD).toBe('[REDACTED]');
  });

  it('handles partial matches in key names', () => {
    const query = {
      access_token: 'token123',
      github_token: 'SOME_TOKEN',
      auth_header: 'Bearer xyz',
    };
    const result = sanitizeQueryParams(query);

    expect(result.access_token).toBe('[REDACTED]');
    expect(result.github_token).toBe('[REDACTED]');
    expect(result.auth_header).toBe('[REDACTED]');
  });

  it('preserves non-sensitive parameters', () => {
    const query = {
      pattern: 'function',
      path: '/src',
      type: 'ts',
      depth: '2',
    };
    const result = sanitizeQueryParams(query);

    expect(result).toEqual(query);
  });

  it('handles empty query object', () => {
    const result = sanitizeQueryParams({});
    expect(result).toEqual({});
  });

  it('handles mixed sensitive and non-sensitive params', () => {
    const query = {
      owner: 'anthropic',
      repo: 'claude',
      token: 'SOME_TOKENxx',
      branch: 'main',
      apikey: 'key123',
    };
    const result = sanitizeQueryParams(query);

    expect(result).toEqual({
      owner: 'anthropic',
      repo: 'claude',
      token: '[REDACTED]',
      branch: 'main',
      apikey: '[REDACTED]',
    });
  });

  it('handles nested values without redacting them', () => {
    const query = {
      config: { token: 'nested' },
      path: '/src',
    };
    const result = sanitizeQueryParams(query);

    expect(result.config).toEqual({ token: 'nested' });
    expect(result.path).toBe('/src');
  });

  it('handles array values', () => {
    const query = {
      patterns: ['search', 'term'],
      token: 'secret',
    };
    const result = sanitizeQueryParams(query);

    expect(result.patterns).toEqual(['search', 'term']);
    expect(result.token).toBe('[REDACTED]');
  });
});
