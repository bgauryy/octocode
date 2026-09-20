import runtimeBinding from './runtime.cjs';

// Mirrors NATIVE_ABI_VERSION in crates/runtime/src/lib.rs.
// Exported here so consumers (e.g. octocode-mcp) can import the expected
// version rather than hardcoding it, while still guarding against a
// mismatched addon loaded via OCTOCODE_NATIVE_BINDING or a stale install.
export const NATIVE_ABI_VERSION = 2;

export const NativeRuntime = runtimeBinding.NativeRuntime;
export default runtimeBinding;
