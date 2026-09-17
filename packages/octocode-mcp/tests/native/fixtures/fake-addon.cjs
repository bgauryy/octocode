// Minimal fake native addon: exports the NativeRuntime constructor the bridge
// requires, so runtime-selection tests can resolve a "loadable" addon without a
// real compiled `.node` binary. CommonJS so createRequire() can load it.
/* eslint-disable no-undef -- CommonJS module scope */
module.exports = {
  NativeRuntime: function NativeRuntime() {},
};
