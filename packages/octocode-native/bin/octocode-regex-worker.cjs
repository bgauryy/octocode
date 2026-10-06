#!/usr/bin/env node
/**
 * Platform-selecting shim for the native `octocode-regex-worker` binary.
 * See bin/octocode.cjs for the full explanation.
 */
'use strict';

require('./launch-native.cjs').launchNativeBinary('octocode-regex-worker');
