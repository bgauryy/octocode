/** Pi-specific naming defaults over the package-owned external-host detector/pool. */
import {
  detectAgentHost as detectExternalAgentHost,
  generateAgentName,
  type AgentHost as ExternalAgentHost,
} from '@octocodeai/octocode-awareness';

export type AgentHost = Exclude<ExternalAgentHost, 'agent'>;

/** Pi falls back to its own `octo` host when no stronger runner signal exists. */
export function detectAgentHost(env: NodeJS.ProcessEnv = process.env): AgentHost {
  const detected = detectExternalAgentHost(env);
  return detected === 'agent' ? 'octo' : detected;
}

export function getRandomAgentName(host: AgentHost = detectAgentHost()): string {
  return generateAgentName({ OCTOCODE_AGENT_HOST: host });
}
