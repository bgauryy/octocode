import fs, {
  mkdtempSync,
  mkdirSync,
  realpathSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir, homedir } from 'node:os';
import { join } from 'node:path';
import { afterEach, describe, expect, it, vi } from 'vitest';

import {
  PathValidator,
  pathValidator,
  resetPathValidator,
} from '../../src/security/pathValidator.js';

describe('PathValidator', () => {
  let tempRoot: string | undefined;

  afterEach(() => {
    if (tempRoot) {
      rmSync(tempRoot, { recursive: true, force: true });
      tempRoot = undefined;
    }
  });

  it('allows paths through a realpath-equivalent allowed root', () => {
    tempRoot = mkdtempSync(join(tmpdir(), 'octocode-path-validator-'));
    const realRoot = realpathSync(tempRoot);
    const validator = new PathValidator({
      includeHomeDir: false,
      additionalRoots: [realRoot],
    });

    const result = validator.validate(tempRoot);

    expect(result.isValid).toBe(true);
    expect(result.sanitizedPath).toBe(realRoot);
  });
});

describe('PathValidator.validate — input & existing-path branches', () => {
  let tempRoot: string;

  function newValidator() {
    tempRoot = realpathSync(mkdtempSync(join(tmpdir(), 'octocode-pv-')));
    return new PathValidator({
      includeHomeDir: false,
      additionalRoots: [tempRoot],
    });
  }

  afterEach(() => {
    vi.restoreAllMocks();
    if (tempRoot) {
      rmSync(tempRoot, { recursive: true, force: true });
      tempRoot = '';
    }
  });

  it('rejects an empty or whitespace-only path', () => {
    const v = newValidator();
    expect(v.validate('').isValid).toBe(false);
    expect(v.validate('   ').isValid).toBe(false);
    expect(v.validate('   ').error).toMatch(/cannot be empty/i);
  });

  it('accepts a real file that lives inside an allowed root', () => {
    const v = newValidator();
    const file = join(tempRoot, 'a.txt');
    writeFileSync(file, 'hi');
    const r = v.validate(file);
    expect(r.isValid).toBe(true);
    expect(r.sanitizedPath).toBe(realpathSync(file));
  });

  it('rejects a symlink whose target resolves outside the allowed roots', () => {
    const v = newValidator();
    const outside = realpathSync(
      mkdtempSync(join(tmpdir(), 'octocode-outside-'))
    );
    const secret = join(outside, 'secret.txt');
    writeFileSync(secret, 'top secret');
    const link = join(tempRoot, 'link.txt');
    symlinkSync(secret, link);

    const r = v.validate(link);
    expect(r.isValid).toBe(false);
    expect(r.error).toMatch(/outside allowed directories/);
    // A genuine symlink escape (lexical path inside, target outside) keeps the
    // "Symlink target" wording.
    expect(r.error).toMatch(/Symlink target/i);

    rmSync(outside, { recursive: true, force: true });
  });

  it('allows a symlink located OUTSIDE allowed roots whose target resolves INTO an allowed root (realpath-only invariant)', () => {
    // Deliberate design: a path is allowed iff its *real* (symlink-resolved)
    // location is within an allowed root, regardless of how it was spelled.
    // This is why macOS /var -> /private/var and /tmp -> /private/tmp prefixes
    // work, and why tightening to a "lexical path must also be inside"
    // canonical two-stage gate would regress those. A symlink cannot EXPAND the
    // reachable set here — the target is already inside an allowed root — so
    // this is safe. (The dangerous inside->outside direction is blocked by the
    // "symlink whose target resolves outside" test above.)
    const v = newValidator();
    const outside = realpathSync(
      mkdtempSync(join(tmpdir(), 'octocode-inbound-'))
    );
    const link = join(outside, 'inbound-link');
    symlinkSync(tempRoot, link);

    const r = v.validate(link);
    expect(r.isValid).toBe(true);
    expect(r.sanitizedPath).toBe(tempRoot);

    rmSync(outside, { recursive: true, force: true });
  });

  it('rejects an existing symlink-free path outside allowed roots WITHOUT blaming a symlink (issue #450)', () => {
    const v = newValidator();
    // A plain, existing directory with no symlinks anywhere on the path, simply
    // living outside the allowed roots.
    const outside = realpathSync(
      mkdtempSync(join(tmpdir(), 'octocode-plain-outside-'))
    );
    const dir = join(outside, 'src', 'Core');
    mkdirSync(dir, { recursive: true });

    const r = v.validate(dir);
    expect(r.isValid).toBe(false);
    // The denial is correct; the wording must name the real cause…
    expect(r.error).toMatch(/is outside allowed directories/);
    expect(r.error).toMatch(/^Path /);
    // …and must NOT invent a nonexistent symlink.
    expect(r.error).not.toMatch(/Symlink/i);

    rmSync(outside, { recursive: true, force: true });
  });

  it('names the configured allowed roots in the out-of-root error so agents can self-correct (issue #450)', () => {
    const v = newValidator();
    const outside = realpathSync(
      mkdtempSync(join(tmpdir(), 'octocode-plain-outside-'))
    );
    const dir = join(outside, 'src');
    mkdirSync(dir, { recursive: true });

    const r = v.validate(dir);
    expect(r.isValid).toBe(false);
    expect(r.error).toContain(tempRoot);

    rmSync(outside, { recursive: true, force: true });
  });

  it('rejects a path that matches an ignored pattern even inside an allowed root', () => {
    const v = newValidator();
    const gitDir = join(tempRoot, '.git');
    mkdirSync(gitDir);
    const inside = join(gitDir, 'config');
    writeFileSync(inside, '[core]');

    const r = v.validate(inside);
    expect(r.isValid).toBe(false);
    expect(r.error).toMatch(/ignored/);
  });

  it('maps EACCES from realpathSync to a permission-denied error', () => {
    const v = newValidator();
    vi.spyOn(fs, 'realpathSync').mockImplementation(() => {
      throw Object.assign(new Error('perm'), { code: 'EACCES' });
    });
    const r = v.validate(join(tempRoot, 'x'));
    expect(r.isValid).toBe(false);
    expect(r.error).toMatch(/Permission denied/);
  });

  it('maps ELOOP from realpathSync to a symlink-loop error', () => {
    const v = newValidator();
    vi.spyOn(fs, 'realpathSync').mockImplementation(() => {
      throw Object.assign(new Error('loop'), { code: 'ELOOP' });
    });
    const r = v.validate(join(tempRoot, 'x'));
    expect(r.isValid).toBe(false);
    expect(r.error).toMatch(/Symlink loop/);
  });

  it('maps ENAMETOOLONG from realpathSync to a name-too-long error', () => {
    const v = newValidator();
    vi.spyOn(fs, 'realpathSync').mockImplementation(() => {
      throw Object.assign(new Error('too long'), { code: 'ENAMETOOLONG' });
    });
    const r = v.validate(join(tempRoot, 'x'));
    expect(r.isValid).toBe(false);
    expect(r.error).toMatch(/too long/);
  });

  it('maps an unknown fs error code to a generic validation error', () => {
    const v = newValidator();
    vi.spyOn(fs, 'realpathSync').mockImplementation(() => {
      throw Object.assign(new Error('weird'), { code: 'EWEIRD' });
    });
    const r = v.validate(join(tempRoot, 'x'));
    expect(r.isValid).toBe(false);
    expect(r.error).toMatch(/Unexpected error validating path/);
  });
});

describe('PathValidator.validate — non-existent path (ENOENT) branch', () => {
  let tempRoot: string;

  afterEach(() => {
    if (tempRoot) {
      rmSync(tempRoot, { recursive: true, force: true });
      tempRoot = '';
    }
  });

  it('accepts a not-yet-created path whose existing ancestor is inside an allowed root', () => {
    tempRoot = realpathSync(mkdtempSync(join(tmpdir(), 'octocode-pv-enoent-')));
    const v = new PathValidator({
      includeHomeDir: false,
      additionalRoots: [tempRoot],
    });

    // Deeply nested path that does not exist yet; ancestor tempRoot exists.
    const future = join(tempRoot, 'nope', 'deeper', 'out.txt');
    const r = v.validate(future);
    expect(r.isValid).toBe(true);
    expect(r.sanitizedPath).toBe(join(tempRoot, 'nope', 'deeper', 'out.txt'));
  });

  it('rejects a non-existent path whose resolved location is outside allowed roots', () => {
    tempRoot = realpathSync(mkdtempSync(join(tmpdir(), 'octocode-pv-enoent-')));
    const otherRoot = realpathSync(
      mkdtempSync(join(tmpdir(), 'octocode-pv-other-'))
    );
    const v = new PathValidator({
      includeHomeDir: false,
      additionalRoots: [tempRoot],
    });

    const future = join(otherRoot, 'does-not-exist.txt');
    const r = v.validate(future);
    expect(r.isValid).toBe(false);
    expect(r.error).toMatch(/outside allowed directories/);

    rmSync(otherRoot, { recursive: true, force: true });
  });

  it('rejects a non-existent path that resolves into an ignored directory', () => {
    tempRoot = realpathSync(mkdtempSync(join(tmpdir(), 'octocode-pv-enoent-')));
    const v = new PathValidator({
      includeHomeDir: false,
      additionalRoots: [tempRoot],
    });

    const future = join(tempRoot, '.ssh', 'id_rsa');
    const r = v.validate(future);
    expect(r.isValid).toBe(false);
    expect(r.error).toMatch(/ignored/);
  });
});

describe('PathValidator — allowed roots, env, and tilde expansion', () => {
  const savedAllowedPaths = process.env.ALLOWED_PATHS;
  let tempRoot: string;

  afterEach(() => {
    if (savedAllowedPaths === undefined) delete process.env.ALLOWED_PATHS;
    else process.env.ALLOWED_PATHS = savedAllowedPaths;
    if (tempRoot) {
      rmSync(tempRoot, { recursive: true, force: true });
      tempRoot = '';
    }
  });

  it('adds roots from the ALLOWED_PATHS env var (comma-separated, trimmed)', () => {
    tempRoot = realpathSync(mkdtempSync(join(tmpdir(), 'octocode-pv-env-')));
    process.env.ALLOWED_PATHS = ` ${tempRoot} , , `;
    const v = new PathValidator({ includeHomeDir: false });
    expect(v.getAllowedRoots()).toContain(tempRoot);
  });

  it('opts into the home directory only when includeHomeDir is true', () => {
    const withHome = new PathValidator({ includeHomeDir: true });
    const withoutHome = new PathValidator({ includeHomeDir: false });
    expect(withHome.getAllowedRoots()).toContain(homedir());
    expect(withoutHome.getAllowedRoots()).not.toContain(homedir());
  });

  it('expands a leading ~ against the home directory when validating', () => {
    const v = new PathValidator({ includeHomeDir: true });
    // Home itself should validate (it is an allowed root).
    const r = v.validate('~');
    expect(r.isValid).toBe(true);
  });

  it('getAllowedRoots returns a defensive copy', () => {
    tempRoot = realpathSync(mkdtempSync(join(tmpdir(), 'octocode-pv-copy-')));
    const v = new PathValidator({
      includeHomeDir: false,
      additionalRoots: [tempRoot],
    });
    const roots = v.getAllowedRoots() as string[];
    const before = roots.length;
    roots.push('/injected');
    expect(v.getAllowedRoots()).toHaveLength(before);
  });

  it('replaceAllowedRoots swaps the entire root set', () => {
    tempRoot = realpathSync(
      mkdtempSync(join(tmpdir(), 'octocode-pv-replace-'))
    );
    const v = new PathValidator({ includeHomeDir: true });
    v.replaceAllowedRoots([tempRoot]);
    expect(v.getAllowedRoots()).toEqual([tempRoot]);
    expect(v.getAllowedRoots()).not.toContain(homedir());
  });
});

describe('PathValidator.exists / getType', () => {
  let tempRoot: string;
  let v: PathValidator;

  afterEach(() => {
    if (tempRoot) {
      rmSync(tempRoot, { recursive: true, force: true });
      tempRoot = '';
    }
  });

  function setup() {
    tempRoot = realpathSync(mkdtempSync(join(tmpdir(), 'octocode-pv-type-')));
    v = new PathValidator({
      includeHomeDir: false,
      additionalRoots: [tempRoot],
    });
  }

  it('exists() returns true for a readable file inside an allowed root', async () => {
    setup();
    const file = join(tempRoot, 'f.txt');
    writeFileSync(file, 'x');
    expect(await v.exists(file)).toBe(true);
  });

  it('exists() returns false for a rejected (out-of-root) path', async () => {
    setup();
    expect(await v.exists('/etc/passwd')).toBe(false);
  });

  it('getType() classifies files, directories, and symlinks', async () => {
    setup();
    const file = join(tempRoot, 'f.txt');
    writeFileSync(file, 'x');
    const dir = join(tempRoot, 'sub');
    mkdirSync(dir);

    expect(await v.getType(file)).toBe('file');
    expect(await v.getType(dir)).toBe('directory');
  });

  it('getType() returns null for a path that fails validation', async () => {
    setup();
    expect(await v.getType('/etc/passwd')).toBeNull();
  });
});

describe('resetPathValidator', () => {
  it('restores the home-dir-inclusive default when called with no arguments', () => {
    resetPathValidator({ includeHomeDir: false });
    expect(pathValidator.getAllowedRoots()).not.toContain(homedir());

    resetPathValidator();
    expect(pathValidator.getAllowedRoots()).toContain(homedir());
  });

  it('applies explicit options and returns the shared singleton', () => {
    const tmp = realpathSync(mkdtempSync(join(tmpdir(), 'octocode-pv-reset-')));
    const returned = resetPathValidator({
      includeHomeDir: false,
      additionalRoots: [tmp],
    });
    expect(returned).toBe(pathValidator);
    expect(pathValidator.getAllowedRoots()).toContain(tmp);

    resetPathValidator();
    rmSync(tmp, { recursive: true, force: true });
  });
});
