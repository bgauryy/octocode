// A loadable module that does NOT export a NativeRuntime constructor, used to
// cover loadNativeBinding's guard (the candidate addon must export NativeRuntime).
/* eslint-disable no-undef -- CommonJS module scope */
module.exports = { somethingElse: true };
