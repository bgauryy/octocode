import assert from 'node:assert/strict';
import { test } from 'vitest';
import {
  NativeErrorCodes,
  NativeOperationError,
} from '@octocodeai/octocode-extension-rust';
import {
  FileMutationConflictError,
  rethrowFileMutationConflict,
} from '../src/tools/file-mutation-target.js';

const target = { requestPath: 'state.txt', canonicalPath: '/workspace/state.txt' };

test('native precondition failures become file mutation conflicts', () => {
  const native = new NativeOperationError(
    NativeErrorCodes.PRECONDITION_FAILED,
    'PRECONDITION_FAILED: snapshot changed',
  );
  assert.throws(
    () => rethrowFileMutationConflict(native, target),
    (error) => error instanceof FileMutationConflictError && error.cause === undefined,
  );
});

test('diagnostic wording alone does not classify an unrelated failure as a conflict', () => {
  const unrelated = new Error('cache changed while formatting a stale status label');
  assert.throws(() => rethrowFileMutationConflict(unrelated, target), (error) => error === unrelated);
});

test('an exact create-only EEXIST failure remains a conflict', () => {
  const exists = Object.assign(new Error('target exists'), { code: 'EEXIST' });
  assert.throws(
    () => rethrowFileMutationConflict(exists, target),
    (error) => error instanceof FileMutationConflictError,
  );
});
