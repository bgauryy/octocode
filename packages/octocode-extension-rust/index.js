import native from './index.cjs';
export const {
  NativeCancellation, NativeErrorCodes, NativeOperationError, nativeErrorCode,
  ensurePrivateDirectory, flushFile, readGitObject, snapshotFile, fingerprintFiles,
  replaceFile, deleteFile, computeLineDiff, computeLineDiffAsync,
  generateDiffArtifacts, generateDiffArtifactsAsync,
} = native;
