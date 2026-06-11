import { describe, expect, it } from 'vitest';
import { getExtension } from '@octocodeai/octocode-minifier';

describe('getExtension', () => {
  // ── normal files ─────────────────────────────────────────────────────────
  it('returns the final extension for multi-dot paths', () => {
    expect(getExtension('archive.backup.JSON')).toBe('JSON');
  });

  it('lowercases the extension when requested', () => {
    expect(getExtension('archive.backup.JSON', { lowercase: true })).toBe(
      'json'
    );
  });

  it('returns the last segment for a simple extension', () => {
    expect(getExtension('index.ts')).toBe('ts');
  });

  it('handles paths with directory segments', () => {
    expect(getExtension('src/utils/file.ts', { lowercase: true })).toBe('ts');
  });

  it('handles deeply nested paths', () => {
    expect(getExtension('a/b/c/d.test.tsx', { lowercase: true })).toBe('tsx');
  });

  // ── extensionless files ──────────────────────────────────────────────────
  it('returns the configured fallback for extensionless paths', () => {
    expect(getExtension('Makefile', { fallback: 'txt' })).toBe('txt');
  });

  it('returns an empty string when no fallback is configured', () => {
    expect(getExtension('README')).toBe('');
  });

  it('returns fallback for extensionless path with directory', () => {
    expect(getExtension('src/Makefile', { fallback: 'txt' })).toBe('txt');
  });

  // ── dotfiles — FIX: must return the extension, NOT the fallback ──────────
  it('.gitignore → "gitignore" (matches MINIFY_CONFIG entry)', () => {
    expect(getExtension('.gitignore', { fallback: 'txt' })).toBe('gitignore');
  });

  it('.gitignore lowercase stays "gitignore"', () => {
    expect(
      getExtension('.gitignore', { lowercase: true, fallback: 'txt' })
    ).toBe('gitignore');
  });

  it('.env → "env"', () => {
    expect(getExtension('.env', { fallback: 'txt' })).toBe('env');
  });

  it('.dockerignore → "dockerignore"', () => {
    expect(getExtension('.dockerignore', { fallback: 'txt' })).toBe(
      'dockerignore'
    );
  });

  it('.npmrc → "npmrc"', () => {
    expect(getExtension('.npmrc', { fallback: 'txt' })).toBe('npmrc');
  });

  it('.eslintrc → "eslintrc"', () => {
    expect(getExtension('.eslintrc', { fallback: 'txt' })).toBe('eslintrc');
  });

  it('.prettierrc → "prettierrc"', () => {
    expect(getExtension('.prettierrc', { fallback: 'txt' })).toBe('prettierrc');
  });

  it('.GITIGNORE uppercased lowercases when lowercase:true', () => {
    expect(getExtension('.GITIGNORE', { lowercase: true })).toBe('gitignore');
  });

  it('dotfile inside a directory path → extension still returned', () => {
    expect(getExtension('home/user/.gitignore', { fallback: 'txt' })).toBe(
      'gitignore'
    );
  });

  it('dotfile with no options → returns ext without lowercasing', () => {
    expect(getExtension('.gitignore')).toBe('gitignore');
  });

  // ── edge cases ───────────────────────────────────────────────────────────
  it('empty string → empty fallback', () => {
    expect(getExtension('')).toBe('');
  });

  it('single dot "." → empty string (dot with no extension)', () => {
    expect(getExtension('.', { fallback: 'txt' })).toBe('');
  });

  it('trailing dot "file." → empty string', () => {
    expect(getExtension('file.')).toBe('');
  });
});
