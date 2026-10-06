#!/usr/bin/env node
/**
 * Platform-selecting bin for `@octocodeai/octocode-native` (e.g. `npx
 * @octocodeai/octocode-native`). Resolves the compiled binary via the shared
 * `resolve-binary.cjs` and spawns it, passing arguments and stdio through.
 *
 * The `octocode` npm CLI does NOT route through this launcher — it resolves the
 * platform binary directly (same resolver) and spawns it, so a tool call costs
 * one Node hop, not two.
 */
'use strict';

require('./launch-native.cjs').launchNativeBinary('octocode');
