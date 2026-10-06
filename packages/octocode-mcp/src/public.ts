// Public entry: the native adapter is the single source for both the runtime
// glue and its types. This module only re-exports it, adding no policy.
export {
  loadNativeBinding,
  createNativeMcp,
  startNativeMcp,
} from './native/index.js';

export type {
  ClassificationProbe,
  NativeRuntime,
  NativeRuntimeOptions,
  NativeRuntimeBinding,
  NativeCatalogTool,
  NativeCatalog,
  NativeMcp,
  NativeMcpOptions,
} from './native/index.js';
