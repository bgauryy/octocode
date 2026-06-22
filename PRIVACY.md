# Privacy Policy

**Effective Date:** January 18, 2026

## 1. Overview

Octocode ("we", "us", or "our") is an open-source project committed to protecting the privacy of developers. This policy explains how we handle data within the Octocode CLI, MCP Server, and VS Code Extension.

## 2. Data Collection & Classification

Octocode does not collect remote usage events from the CLI, MCP Server, or VS Code Extension.

### What We Collect
- **Local runtime state**: Configuration, encrypted credentials, sessions, stats, and temporary materialization caches may be stored on your machine to run the tools.
- **Authentication data you provide**: Tokens or credentials you configure are stored locally using the supported credential storage path for your platform.

### What We DO NOT Collect
- **Source Code**: Your code stays on your machine. We never upload or "peek" at your local files.
- **Secrets & Env Vars**: We do not collect API keys, passwords, or environment variables.
- **PII**: We do not collect names or emails through product usage reporting.

## 3. Legal Basis (GDPR/CCPA)

For users in the EEA and UK, local product state remains under your control unless you choose to send data to an external service, such as GitHub, npm, or a third-party LLM provider. Those services process data under their own terms and privacy policies.

## 4. Data Retention

Octocode does not retain remote usage records. Local runtime state remains on your machine until you remove it.

## 5. Consent and Data Disputes

### 5.1 External Services
For features involving external services, you control which credentials and providers are configured. Data sent to those services is governed by their respective policies.

### 5.2 The Right to Contest
If you believe your data has been collected, stored, or processed in a way that violates this policy or your local privacy laws (such as GDPR or CCPA), you have the right to contest our practices.

**To File a Contest:**
1. **Email:** Contact us at bgauryy@octocodeai.com with the subject "Privacy Contest."
2. **Details:** Provide your Session ID (if known) or a description of the tool usage in question.
3. **Resolution:** We commit to acknowledging your contest within 72 hours and providing a full written resolution within 15 business days.

## 6. Your Rights

Under global privacy laws, you have the right to:
- **Access/Export**: Request information about any data you believe was shared with Octocode maintainers.
- **Deletion**: Request deletion of any data you believe was shared with Octocode maintainers.
- **Provider Control**: Remove configured credentials or stop using external providers at any time.

## 7. AI Transparency (EU AI Act Compliance)
Octocode functions as an AI-orchestration layer.
* **Model Privacy:** We do not use your prompts or source code to train Octocode-owned models.
* **Third-Party LLMs:** When you use external providers (e.g., OpenAI, Anthropic), your data is governed by their respective privacy policies. Octocode does not store the content of these external AI interactions.

## 8. Contact & Issues

For privacy inquiries or to exercise your data rights, please open a Privacy Issue on our GitHub repository or contact the maintainers at bgauryy@octocodeai.com.
