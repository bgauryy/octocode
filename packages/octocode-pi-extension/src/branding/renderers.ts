/**
 * Octocode branded TUI renderers for Pi extension tool rows.
 *
 * Wrap point: `registerUniqueTool` in src/tools/octocode-tools.ts — the single
 * funnel through which every Octocode extension tool registration passes. Decorating
 * there means zero per-tool changes and zero risk of missing a future tool.
 *
 * Strategy: preserve each tool's single-operation renderer, but wrap all call
 * renderers into the same operation/reasoning blocks and all ordered batch
 * outcomes into the same compact per-query result rows.
 */

import {
  buildOctocodeRenderCall,
  buildOctocodeRenderResult,
  buildQueryCallBlocks,
  buildQueryResultRows,
  extractQueryResultRows,
} from '../tools/render-helpers.js';
import type { ToolDefinition, PiTheme, RenderContext, ToolCallResult, RenderResultOptions } from '../types.js';

// ─── Public API ───────────────────────────────────────────────────────────────

export interface WithOctocodeRenderOpts {
  /**
   * Override the display name shown in the title line.
   * Defaults to `def.name`.
   */
  displayName?: string;
}

/**
 * Decorator that normalizes per-query call/results while retaining each tool's
 * existing single-operation renderer and rich single-query output.
 *
 * Signature: `withOctocodeRender(def, opts?) → def`
 *
 * The returned object is the same reference with slots filled in-place so
 * downstream code that holds the reference sees the updated renderers.
 */
export function withOctocodeRender<T extends ToolDefinition>(
  def: T,
  opts: WithOctocodeRenderOpts = {},
): T {
  const displayName = opts.displayName ?? def.name;

  const ownCall = def.renderCall;
  def.renderCall = function brandedRenderCall(
    args: unknown,
    theme?: PiTheme,
    context?: RenderContext,
  ) {
      if (ownCall) {
        return buildQueryCallBlocks(
          args,
          theme,
          (singleArgs) => ownCall(singleArgs, theme, context),
        );
      }
      return buildOctocodeRenderCall(displayName, args, theme);
    };

  if (!def.renderResult) {
    def.renderResult = function brandedRenderResult(
      result: ToolCallResult,
      opts: RenderResultOptions,
      theme?: PiTheme,
      context?: RenderContext,
    ) {
      return buildOctocodeRenderResult(displayName, result, opts, theme, context);
    };
  } else {
    // A tool with its OWN renderResult still keys its error styling off the
    // returned result.isError. But Pi IGNORES that flag and instead sets a
    // system-level context.isError when execute() threw or the call was rejected
    // (e.g. arg-schema validation) — cases where the tool's own renderer would
    // paint a misleading success/empty row (see the empty red MCP rows bug).
    // Wrap it: on a system error with no tool-produced error result, show the
    // uniform branded error row (which surfaces the failure text); otherwise
    // delegate unchanged so each tool keeps its bespoke success/error rendering.
    const own = def.renderResult;
    def.renderResult = function guardedRenderResult(
      result: ToolCallResult,
      opts: RenderResultOptions,
      theme?: PiTheme,
      context?: RenderContext,
    ) {
      if (!opts?.isPartial && context?.isError && !result?.isError) {
        return buildOctocodeRenderResult(displayName, result, opts, theme, context);
      }
      // Skip the shared query-row fallback when the tool's own renderer declares
    // it handles multi-query output itself (marked via renderResult.multiQueryAware).
    if (
        extractQueryResultRows(result).length > 1 &&
        !(own as { multiQueryAware?: boolean }).multiQueryAware
      ) {
        return buildQueryResultRows(displayName, result, theme)!;
      }
      return own(result, opts, theme, context);
    };
  }

  return def;
}
