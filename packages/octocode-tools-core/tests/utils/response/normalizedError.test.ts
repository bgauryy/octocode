import { describe, expect, it } from 'vitest';
import { normalizeError } from '../../../src/utils/response/normalizedError.js';

describe('normalizeError', () => {
  it('handles an Error instance', () => {
    const err = new Error('boom');
    const result = normalizeError(err);
    expect(result.message).toBe('boom');
    expect(result.name).toBe('Error');
    expect(result.code).toBeUndefined();
  });

  it('falls back to Error name when error.name is empty', () => {
    const err = new Error('boom');
    Object.defineProperty(err, 'name', { value: '' });
    expect(normalizeError(err).name).toBe('Error');
  });

  it('falls back to String(error) when error.message is empty', () => {
    const err = new Error('');
    const result = normalizeError(err);
    // message is falsy — falls back to String(err)
    expect(result.message).toBeDefined();
  });

  it('extracts string code from Error instance', () => {
    const err = Object.assign(new Error('oops'), { code: 'ENOENT' });
    const result = normalizeError(err);
    expect(result.code).toBe('ENOENT');
  });

  it('ignores non-string code on Error instance', () => {
    const err = Object.assign(new Error('oops'), { code: 42 });
    expect(normalizeError(err).code).toBeUndefined();
  });

  it('handles a plain string', () => {
    const result = normalizeError('something went wrong');
    expect(result.name).toBe('Error');
    expect(result.message).toBe('something went wrong');
    expect(result.code).toBeUndefined();
  });

  it('handles a plain object with message and name', () => {
    const result = normalizeError({ name: 'CustomError', message: 'bad', code: 'ERR_X' });
    expect(result.name).toBe('CustomError');
    expect(result.message).toBe('bad');
    expect(result.code).toBe('ERR_X');
  });

  it('handles a plain object without message — serializes to JSON', () => {
    const result = normalizeError({ foo: 'bar' });
    expect(result.message).toBe('{"foo":"bar"}');
    expect(result.name).toBe('Error');
  });

  it('falls back to Unknown error for non-serializable objects', () => {
    const circular: Record<string, unknown> = {};
    circular.self = circular;
    const result = normalizeError(circular);
    expect(result.message).toBe('Unknown error');
  });

  it('handles undefined', () => {
    const result = normalizeError(undefined);
    expect(result.message).toBe('undefined');
    expect(result.name).toBe('Error');
  });

  it('handles null', () => {
    const result = normalizeError(null);
    expect(result.message).toBe('null');
  });

  it('handles a number', () => {
    const result = normalizeError(404);
    expect(result.message).toBe('404');
  });
});
