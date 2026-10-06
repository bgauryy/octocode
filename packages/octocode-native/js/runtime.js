import runtimeBinding from './runtime.cjs';

// Mirrors NATIVE_ABI_VERSION in crates/runtime/src/lib.rs.
// Exported here so consumers (e.g. octocode-mcp) can import the expected
// version rather than hardcoding it, while still guarding against a stale
// install or a candidate addon loaded through octocode-mcp's dev-only
// OCTOCODE_NATIVE_BINDING override (this package's loader never reads it).
export const NATIVE_ABI_VERSION = 4;

export const NativeRuntime = runtimeBinding.NativeRuntime;
export default runtimeBinding;
