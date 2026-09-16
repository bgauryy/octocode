import type { AuthInfo } from '@modelcontextprotocol/server';
import type { ProcessedBulkResult } from '../types/toolResults.js';
import { getProvider } from '../providers/factory.js';
import {
  isProviderSuccess,
  type ICodeHostProvider,
  type ProviderCapabilities,
  type ProviderResponse,
  type ProviderType,
} from '../providers/types.js';
import { getActiveProvider, getActiveProviderConfig } from '../serverConfig.js';
import { maskSensitiveData } from '../security/sanitize.js';
import { handleProviderError } from './utils.js';

export interface ProviderExecutionContext {
  providerType: ProviderType;
  provider: ICodeHostProvider;
  capabilities: ProviderCapabilities;
  baseUrl?: string;
  token?: string;
  authInfo?: AuthInfo;
}

export class ProviderInitializationError extends Error {
  readonly providerType: ProviderType;

  constructor(providerType: ProviderType, message: string) {
    super(message);
    this.name = 'ProviderInitializationError';
    this.providerType = providerType;
  }
}

export function createProviderExecutionContext(
  authInfo?: AuthInfo
): ProviderExecutionContext {
  const activeProviderConfig = getActiveProviderConfig();
  const providerType = activeProviderConfig.provider ?? getActiveProvider();
  const { baseUrl, token } = activeProviderConfig;

  try {
    const provider = getProvider(providerType, {
      type: providerType,
      baseUrl,
      token,
      authInfo,
    });

    return {
      providerType,
      provider,
      capabilities: provider.capabilities,
      baseUrl,
      token,
      authInfo,
    };
  } catch (error) {
    const errorMessage =
      error instanceof Error ? error.message : 'Unknown provider error';
    const sanitizedMessage = maskSensitiveData(errorMessage);
    throw new ProviderInitializationError(
      providerType,
      `Failed to initialize ${providerType} provider: ${sanitizedMessage}`
    );
  }
}

export function createLazyProviderContext(
  authInfo?: AuthInfo
): () => ProviderExecutionContext {
  let ctx: ProviderExecutionContext | undefined;
  return () => (ctx ??= createProviderExecutionContext(authInfo));
}

export function providerSupports(
  context: Pick<ProviderExecutionContext, 'capabilities'>,
  capability: keyof ProviderCapabilities
): boolean {
  return context.capabilities[capability];
}

export async function executeProviderOperation<
  TQuery extends {
    goal?: string;
    reasoning?: string;
  },
  TData,
>(
  query: TQuery,
  operation: () => Promise<ProviderResponse<TData>>
): Promise<
  | { ok: true; response: ProviderResponse<TData> & { data: TData } }
  | { ok: false; result: ProcessedBulkResult }
> {
  const response = await operation();

  if (!isProviderSuccess(response)) {
    return {
      ok: false,
      result: handleProviderError(response, query),
    };
  }

  return {
    ok: true,
    response,
  };
}
