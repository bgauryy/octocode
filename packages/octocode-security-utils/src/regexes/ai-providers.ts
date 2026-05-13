import type { SensitiveDataPattern } from './types.js';

export const aiProviderPatterns: SensitiveDataPattern[] = [
  // OpenAI - Legacy format (pre-2024)
  {
    name: 'openaiApiKeyLegacy',
    description: 'OpenAI API key (legacy format)',
    regex: /\b(sk-[a-zA-Z0-9_-]+T3BlbkFJ[a-zA-Z0-9_-]+)\b/g,
    matchAccuracy: 'high',
  },
  // OpenAI - Project-scoped keys (current format since 2024)
  {
    name: 'openaiProjectApiKey',
    description: 'OpenAI project-scoped API key',
    regex: /\bsk-proj-[a-zA-Z0-9_-]{20,}\b/g,
    matchAccuracy: 'high',
  },
  // OpenAI - Service account keys
  {
    name: 'openaiServiceAccountKey',
    description: 'OpenAI service account API key',
    regex: /\bsk-svcacct-[a-zA-Z0-9_-]{20,}\b/g,
    matchAccuracy: 'high',
  },
  // OpenAI - Admin keys
  {
    name: 'openaiAdminKey',
    description: 'OpenAI admin API key',
    regex: /\bsk-admin-[a-zA-Z0-9_-]{20,}\b/g,
    matchAccuracy: 'high',
  },
  {
    name: 'openaiOrgId',
    description: 'OpenAI organization ID',
    regex: /\borg-[a-zA-Z0-9]{20,}\b/g,
    matchAccuracy: 'high',
  },
  // Groq
  {
    name: 'groqApiKey',
    description: 'Groq API key',
    regex: /\bgsk_[a-zA-Z0-9-_]{51,52}\b/g,
    matchAccuracy: 'high',
  },

  // Cohere
  {
    name: 'cohereApiKey',
    description: 'Cohere API key',
    regex: /\bco-[a-zA-Z0-9-_]{38,64}\b/g,
    matchAccuracy: 'high',
  },

  // Hugging Face
  {
    name: 'huggingFaceToken',
    description: 'Hugging Face API token',
    regex: /\bhf_[a-zA-Z0-9]{34}\b/g,
    matchAccuracy: 'high',
  },

  // Perplexity
  {
    name: 'perplexityApiKey',
    description: 'Perplexity AI API key',
    regex: /\bpplx-[a-zA-Z0-9]{30,64}\b/g,
    matchAccuracy: 'high',
  },

  // Replicate
  {
    name: 'replicateApiToken',
    description: 'Replicate API token',
    regex: /\br8_[a-zA-Z0-9]{30,}\b/g,
    matchAccuracy: 'high',
  },

  // Anthropic (Claude) - relaxed length/suffix constraint
  {
    name: 'anthropicApiKey',
    description: 'Anthropic API key',
    regex: /\bsk-ant-(?:admin01|api03|sid01)-[\w-]{80,120}\b/g,
    matchAccuracy: 'high',
  },
  // Mistral AI
  {
    name: 'mistralApiKey',
    description: 'Mistral AI API key',
    regex: /\b(?:mistral-|mist_)[a-zA-Z0-9]{32,}\b/g,
    matchAccuracy: 'high',
  },
  // Tavily
  {
    name: 'tavilyApiKey',
    description: 'Tavily API key',
    regex: /\btvly-[a-zA-Z0-9]{30,}\b/g,
    matchAccuracy: 'high',
  },
  // DeepSeek (context-based to avoid false positives with generic sk- prefix)
  {
    name: 'deepseekApiKey',
    description: 'DeepSeek API key',
    regex:
      /\b['"]?(?:DEEPSEEK|deepseek|DeepSeek)_?(?:API|api)?_?(?:KEY|key)['"]?\s*(?::|=>|=)\s*['"]?sk-[a-zA-Z0-9]{32,64}['"]?\b/g,
    matchAccuracy: 'medium',
  },
  // Together AI
  {
    name: 'togetherApiKey',
    description: 'Together AI API key',
    regex:
      /\b['"]?(?:TOGETHER|together)_?(?:API|api)?_?(?:KEY|key)['"]?\s*(?::|=>|=)\s*['"]?[a-zA-Z0-9]{40,64}['"]?\b/g,
    matchAccuracy: 'medium',
  },
  // Fireworks AI
  {
    name: 'fireworksApiKey',
    description: 'Fireworks AI API key',
    regex:
      /\b['"]?(?:FIREWORKS|fireworks)_?(?:API|api)?_?(?:KEY|key)['"]?\s*(?::|=>|=)\s*['"]?[a-zA-Z0-9]{40,64}['"]?\b/g,
    matchAccuracy: 'medium',
  },
  // xAI (Grok)
  {
    name: 'xaiApiKey',
    description: 'xAI (Grok) API key',
    regex: /\bxai-[a-zA-Z0-9]{48,}\b/g,
    matchAccuracy: 'high',
  },
  // OpenRouter
  {
    name: 'openRouterApiKey',
    description: 'OpenRouter API key',
    regex: /\bsk-or-v1-[a-zA-Z0-9]{64}\b/g,
    matchAccuracy: 'high',
  },
  // Amazon Bedrock
  {
    name: 'amazonBedrockApiKey',
    description: 'Amazon Bedrock API key',
    regex: /\bABSK[A-Za-z0-9+/]{109,269}={0,2}\b/g,
    matchAccuracy: 'high',
  },
  // AI21 Labs
  {
    name: 'ai21ApiKey',
    description: 'AI21 Labs API key',
    regex:
      /\b['"]?(?:AI21|ai21)_?(?:API|api)?_?(?:KEY|key)['"]?\s*(?::|=>|=)\s*['"]?[a-zA-Z0-9]{40,64}['"]?\b/g,
    matchAccuracy: 'medium',
  },
  // Stability AI (context-based to avoid false positives with generic sk- prefix)
  {
    name: 'stabilityApiKey',
    description: 'Stability AI API key',
    regex:
      /\b['"]?(?:STABILITY|stability|Stability)_?(?:AI|ai)?_?(?:API|api)?_?(?:KEY|key)['"]?\s*(?::|=>|=)\s*['"]?sk-[a-zA-Z0-9]{48,}['"]?\b/g,
    matchAccuracy: 'medium',
  },
  // Voyage AI
  {
    name: 'voyageApiKey',
    description: 'Voyage AI API key',
    regex: /\bpa-[a-zA-Z0-9]{40,}\b/g,
    matchAccuracy: 'high',
  },

  // --- New AI Provider Patterns ---

  // ElevenLabs (xi- is the HTTP header name, not key prefix; keys are alphanumeric; detect via context)
  {
    name: 'elevenLabsApiKey',
    description: 'ElevenLabs API key (context-based detection)',
    regex:
      /\b['"]?(?:ELEVENLABS|elevenlabs)_?(?:API|api)?_?(?:KEY|key)['"]?\s*(?::|=>|=)\s*['"]?[a-zA-Z0-9]{32,}['"]?\b/gi,
    matchAccuracy: 'medium',
  },

  // Assemblyai
  {
    name: 'assemblyaiApiKey',
    description: 'AssemblyAI API key',
    regex:
      /\b['"]?(?:ASSEMBLYAI|assemblyai|AssemblyAI)_?(?:API|api)?_?(?:KEY|key)['"]?\s*(?::|=>|=)\s*['"]?[a-f0-9]{32}['"]?\b/g,
    matchAccuracy: 'medium',
  },

  // Pinecone (prefix-based - newer format)
  {
    name: 'pineconeApiKeyPrefixed',
    description: 'Pinecone API key (prefixed format)',
    regex: /\bpcsk_[a-zA-Z0-9_]{50,}\b/g,
    matchAccuracy: 'high',
  },

  // Weights & Biases (wandb)
  {
    name: 'wandbApiKey',
    description: 'Weights & Biases API key',
    regex: /\b[a-f0-9]{40}\b/g,
    matchAccuracy: 'medium',
    fileContext: /wandb/i,
  },

  // Comet ML
  {
    name: 'cometApiKey',
    description: 'Comet ML API key',
    regex:
      /\b['"]?(?:COMET|comet)_?(?:API|api)?_?(?:KEY|key)['"]?\s*(?::|=>|=)\s*['"]?[a-zA-Z0-9]{32,64}['"]?\b/g,
    matchAccuracy: 'medium',
  },

  // Langchain / LangSmith
  {
    name: 'langchainApiKey',
    description: 'LangChain/LangSmith API key',
    regex: /\blsv2_[a-zA-Z0-9_]{20,}\b/g,
    matchAccuracy: 'high',
  },

  // Unstructured.io
  {
    name: 'unstructuredApiKey',
    description: 'Unstructured.io API key',
    regex:
      /\b['"]?(?:UNSTRUCTURED|unstructured)_?(?:API|api)?_?(?:KEY|key)['"]?\s*(?::|=>|=)\s*['"]?[a-zA-Z0-9]{32,}['"]?\b/g,
    matchAccuracy: 'medium',
  },
];
