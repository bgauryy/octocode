'use strict';
const assert = require('node:assert/strict');
const { getPlatformSuffix } = require('./platform.cjs');
assert.match(
  getPlatformSuffix() ?? '',
  /^(darwin-(arm64|x64)|linux-(arm64-gnu|x64-(gnu|musl))|win32-x64-msvc)$/
);
console.log('platform detection ok');
