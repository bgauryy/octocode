# Privacy Policy

**Effective Date:** January 18, 2026

## 1. Overview

Octocode ("we", "us", or "our") is an open-source project committed to protecting the privacy of developers. This policy explains how we handle data within the Octocode CLI, MCP Server, and VS Code Extension.

## 2. Data Collection & Classification

We collect **de-identified telemetry data** to maintain the stability and performance of our tools. This data is pseudonymous: it allows us to see trends without identifying you personally.

### What We Collect
- **Command Usage**: Which commands (e.g., `octocode/research`, `octocode/plan`) are executed.
- **Tool Usage**: Which specific tools (e.g., `ghSearchCode`) are utilized.
- **Performance Metrics**: Execution time, success/failure rates, and error codes.
- **Session IDs**: Randomly generated UUIDs used to group related events within a single session.

### What We DO NOT Collect
- **Source Code**: Your code stays on your machine. We never upload or "peek" at your local files.
- **Secrets & Env Vars**: We do not collect API keys, passwords, or environment variables.
- **PII**: We do not collect names, emails

## 3. Legal Basis (GDPR/CCPA)

For users in the EEA and UK, we process data based on **Legitimate Interest** (Article 6(1)(f) GDPR). This allows us to monitor the health of the open-source tool and provide a stable experience for the community.

## 4. Data Retention

We retain telemetry logs for a maximum of **90 days**. After this period, data is either permanently deleted or aggregated into high-level statistics that contain no session identifiers.

## 5. Consent and Data Disputes

### 5.1 Affirmative Consent
By using Octocode, you consent to the collection of de-identified telemetry as outlined in Section 2. For features involving external AI processing, we will provide a one-time "Opt-In" prompt to ensure you are aware of data flows before they occur. You may withdraw your consent at any time through the methods listed in Section 7.

### 5.2 The Right to Contest
If you believe your data has been collected, stored, or processed in a way that violates this policy or your local privacy laws (such as GDPR or CCPA), you have the right to contest our practices. 

**To File a Contest:**
1. **Email:** Contact us at bgauryy@octocodeai.com with the subject "Privacy Contest."
2. **Details:** Provide your Session ID (if known) or a description of the tool usage in question.
3. **Resolution:** We commit to acknowledging your contest within 72 hours and providing a full written resolution within 15 business days.

## 6. Your Rights

Under global privacy laws, you have the right to:
- **Access/Export**: Request a copy of the telemetry data associated with your session ID.
- **Deletion**: Request that your session data be purged from our logs.
- **Opt-Out**: Disable all future collection at any time.

## 7. How to Opt-Out

You can disable telemetry by using any of the following methods:

**Environment Variable:**
```bash
export LOG=false
```

**Config File:**
Set `telemetry: false` in your `.octocoderc` file.

**VS Code Settings:**
Disable "Octocode: Enable Telemetry" in the extension settings.

## 8. AI Transparency (EU AI Act Compliance)
Octocode functions as an AI-orchestration layer. 
* **Model Privacy:** We do not use your telemetry, prompts, or source code to train Octocode-owned models.
* **Third-Party LLMs:** When you use external providers (e.g., OpenAI, Anthropic), your data is governed by their respective privacy policies. Octocode does not store the content of these external AI interactions.

## 9. Contact & Issues

For privacy inquiries or to exercise your data rights, please open a Privacy Issue on our GitHub repository or contact the maintainers at bgauryy@octocodeai.com.
