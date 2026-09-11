'use strict';

const { join } = require('node:path');
const { existsSync } = require('node:fs');
const { currentPlatform } = require('./platforms.cjs');
const platform = currentPlatform();
let native;
try {
  const local = join(__dirname, platform.binary);
  native = require(existsSync(local) ? local : platform.packageName);
} catch (cause) {
  throw new Error(`NATIVE_UNAVAILABLE: Cannot load ${platform.binary}. Reinstall @octocodeai/octocode-extension-rust with optional dependencies enabled (${platform.packageName}). Local development builds use the package build script.`, { cause });
}

const NativeErrorCodes = Object.freeze({
  CANCELLED: 'CANCELLED',
  HISTORY_OBJECT_HASH_MISMATCH: 'HISTORY_OBJECT_HASH_MISMATCH',
  HISTORY_OBJECT_INVALID: 'HISTORY_OBJECT_INVALID',
  HISTORY_OBJECT_LIMIT: 'HISTORY_OBJECT_LIMIT',
  HISTORY_OBJECT_TIME_LIMIT: 'HISTORY_OBJECT_TIME_LIMIT',
  HISTORY_OBJECT_UNAVAILABLE: 'HISTORY_OBJECT_UNAVAILABLE',
  INVALID_MODE: 'INVALID_MODE',
  INVALID_PATH: 'INVALID_PATH',
  INVALID_TEXT: 'INVALID_TEXT',
  IO_FAILURE: 'IO_FAILURE',
  NOT_REGULAR_FILE: 'NOT_REGULAR_FILE',
  PRECONDITION_FAILED: 'PRECONDITION_FAILED',
  TOO_LARGE: 'TOO_LARGE',
  UNSAFE_PATH: 'UNSAFE_PATH',
  UNSUPPORTED_PLATFORM: 'UNSUPPORTED_PLATFORM',
});
const knownNativeErrorCodes = new Set(Object.values(NativeErrorCodes));

function nativeErrorCode(error) {
  if (error && typeof error === 'object' && knownNativeErrorCodes.has(error.code)) return error.code;
  const message = error instanceof Error ? error.message : typeof error === 'string' ? error : '';
  const code = /^([A-Z][A-Z0-9_]*):(?:\s|$)/.exec(message)?.[1];
  return knownNativeErrorCodes.has(code) ? code : undefined;
}

class NativeOperationError extends Error {
  constructor(code, message, cause) {
    super(message, { cause });
    this.name = 'NativeOperationError';
    this.code = code;
  }
}

function normalizeNativeError(cause) {
  if (cause instanceof NativeOperationError) return cause;
  const code = nativeErrorCode(cause);
  if (!code) return cause;
  const message = cause instanceof Error ? cause.message : String(cause);
  return new NativeOperationError(code, message, cause);
}

async function callNative(run) {
  try { return await run(); }
  catch (cause) { throw normalizeNativeError(cause); }
}

function callNativeSync(run) {
  try { return run(); }
  catch (cause) { throw normalizeNativeError(cause); }
}

function maximum(value) {
  if (!Number.isSafeInteger(value) || value < 0 || value > 0xffffffff) {
    throw new TypeError('maxBytes must be an integer between 0 and 4294967295');
  }
  return value;
}

function wellFormedText(value, label, code = 'INVALID_TEXT') {
  if (typeof value !== 'string' || !value.isWellFormed()) {
    throw new TypeError(`${code}: ${label} must be a well-formed Unicode string`);
  }
  return value;
}

const pathText = value => wellFormedText(value, 'path', 'INVALID_PATH');

exports.NativeCancellation = native.NativeCancellation;
exports.NativeErrorCodes = NativeErrorCodes;
exports.NativeOperationError = NativeOperationError;
exports.nativeErrorCode = nativeErrorCode;
exports.flushFile = (path, cancellation) => callNative(() => native.flushFile(pathText(path), cancellation));
exports.ensurePrivateDirectory = (path, cancellation) => callNative(() => native.ensurePrivateDirectory(pathText(path), cancellation));
exports.readGitObject = (path, oid, maxDecodedBytes, maxCompressedBytes, timeoutMs, includeContent = true, cancellation) => callNative(() => {
  if (typeof oid !== 'string' || !/^[0-9a-f]{40}$/.test(oid)) throw new TypeError('HISTORY_OBJECT_INVALID: expected lowercase SHA-1 object id');
  return native.readGitObject(pathText(path), oid, maximum(maxDecodedBytes), maximum(maxCompressedBytes), maximum(timeoutMs), includeContent, cancellation);
});
exports.snapshotFile = (path, maxBytes, includeContent, allowLeafSymlink = false, cancellation) =>
  callNative(() => native.snapshotFile(pathText(path), maximum(maxBytes), includeContent, allowLeafSymlink, cancellation));
exports.fingerprintFiles = (root, paths, maxFileBytes, maxBatchBytes, maxFiles, timeoutMs, cancellation) => callNative(() => {
  pathText(root);
  if (!Array.isArray(paths)) throw new TypeError('paths must be an array');
  paths.forEach(pathText);
  return native.fingerprintFiles(root, paths, maximum(maxFileBytes), maximum(maxBatchBytes), maximum(maxFiles), maximum(timeoutMs), cancellation);
});
exports.replaceFile = (path, content, expectedVersion, maxBytes, createMode, cancellation, parentMode) => callNative(() => {
  pathText(path);
  if (!Buffer.isBuffer(content)) throw new TypeError('content must be a Buffer');
  if (createMode !== undefined && (!Number.isInteger(createMode) || createMode < 0 || createMode > 0o7777)) {
    throw new TypeError('createMode must be an integer between 0 and 4095');
  }
  if (parentMode !== undefined && parentMode !== 0o700) throw new TypeError('parentMode supports only 0700; omit for inherited defaults');
  maximum(maxBytes);
  if (content.length > maxBytes) throw new RangeError(`TOO_LARGE: File exceeds maximum ${maxBytes} bytes`);
  return native.replaceFile(path, content, expectedVersion, maxBytes, createMode, cancellation, parentMode);
});
exports.deleteFile = (path, expectedVersion, maxBytes, cancellation) =>
  callNative(() => native.deleteFile(pathText(path), expectedVersion, maximum(maxBytes), cancellation));
exports.computeLineDiff = (oldText, newText) =>
  callNativeSync(() => native.computeLineDiff(wellFormedText(oldText, 'oldText'), wellFormedText(newText, 'newText')));
exports.computeLineDiffAsync = (oldText, newText) =>
  callNative(() => native.computeLineDiffAsync(wellFormedText(oldText, 'oldText'), wellFormedText(newText, 'newText')));
exports.generateDiffArtifactsAsync = (filePath, oldText, newText) =>
  callNative(() => native.generateDiffArtifactsAsync(pathText(filePath), wellFormedText(oldText, 'oldText'), wellFormedText(newText, 'newText')));
exports.generateDiffArtifacts = (filePath, oldText, newText) =>
  callNativeSync(() => native.generateDiffArtifacts(pathText(filePath), wellFormedText(oldText, 'oldText'), wellFormedText(newText, 'newText')));
