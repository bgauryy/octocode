#!/usr/bin/env node
/**
 * Pre-pack guard for octocode-mcp.
 *
 * The runtime addon is distributed through optional platform dependencies
 * owned by @octocodeai/octocode-native. MCP resolves only the native
 * `/runtime` entrypoint, without a TypeScript execution fallback.
 */
console.error('✓ octocode-mcp prepack: npm runtime assets are dependency-owned.');
