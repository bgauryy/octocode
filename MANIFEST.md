# Manifest of Octocode for Research Driven Development

> **Disclaimer**: This is an ongoing research document representing the main vision of Octocode. Algorithms and decisions might change; this document serves to demonstrate the main idea behind Octocode.
>
> Octocode intends to make sufficient simplified research using minimal resources to gather data with minimal noise.

## Overview
This document serves as the **Manifest of Octocode for Research Driven Development (RDD)**. It introduces the methodology, the concept of **"Vibe-Research"**, the definition of **"Smart Research"**, and the **Process Context Oriented Flows** that drive high-quality software development. By leveraging Octocode's research capabilities, we shift from "guess-driven" to "research-driven" development without breaking your flow.

---

## Part 1: The Core Philosophy

### What is Research Driven Development (RDD)?
**Research Driven Development (RDD)** is the methodology that formalizes "vibe-research." It prioritizes **evidence gathering** and **context validation** before any code is implemented. The core philosophy is simple: **Code is Truth, but Context is the Map.**

### What is "Vibe-Research"?
"Vibe-research" is the **intuitive flow state** enabled by Octocode's research engine. It transforms the often tedious process of gathering context into a seamless, conversational rhythm. Instead of context-switching, you stay in the "vibe" while Octocode handles the forensics.

### What is "Smart Research"?
"Smart Research" is the automated, evidence-based forensics capability of the Octocode engine. It bridges the gap between intuition ("vibe") and reality (code) by intelligently deciding which tools to use—traversing call graphs, searching external repos, or validating assumptions—without manual micromanagement.

*   **The Vibe**: "I feel like this function is related to the auth service..."
*   **The Research**: Octocode instantly validates that feeling using actual evidence using research tools
*   **The Result**: You move from "guessing" to "knowing" without breaking your coding stride.

### The RDD Equation
$$ RDD = (Static Context + Dynamic Context) \times Validation \times \epsilon$$

*   **Static Context (The Knowns)**: The immutable truth of your current code (`octocode-local`).
*   **Dynamic Context (The Unknowns)**: External knowledge, history, and patterns (`octocode-external`).
*   **Validation (The Proof)**: Cross-referencing against reality to ensure your map matches the territory.

---

## Part 2: Process Context Oriented Flows (The Workflow)

### Multi-Agent Session Architecture (GAN-Inspired)
This workflow draws inspiration from **Generative Adversarial Networks (GANs)**, where a "Generator" (the doing agent) and a "Discriminator" (the validating agent) compete to ensure maximum quality.

#### Latency vs. Quality: The Trade-off
The full GAN-inspired flow (6 steps with cross-model validation) is logically sound but computationally expensive. To address this, we define two operational modes:

1.  **Fast-Path**: For simple tasks, the "Verifier" step is optional or lighter (self-correction).
2.  **Deep-Research Path**: For complex architectural changes, the full adversarial loop is enforced.

This distinction ensures we don't over-engineer simple fixes while maintaining rigor for critical changes.

```
┌────────────────────────────────────────────────────────────────────────┐
│                        RDD ADVERSARIAL FLOW                            │
├────────────────────────────────────────────────────────────────────────┤
│                                                                        │
│  0. INIT RESEARCH     1. PLAN              2. VERIFY                   │
│  ┌───────────┐      ┌───────────┐       ┌───────────┐                  │
│  │ Researcher│─────►│ Planner   │─────►│ Verifier  │                  │
│  │ (Ctx)     │      │ (Gen)     │       │ (Disc)    │                  │
│  └───────────┘      └───────────┘       └───────────┘                  │
│       │                  │                    │                        │
│       ▼                  ▼                    ▼                        │
│   [init-ctx]         [plan.md]           [plan.md']                    │
│                                                                        │
│  3. RESEARCH         4. VALIDATE                                       │
│  ┌───────────┐      ┌───────────┐                                      │
│  │ Researcher│─────►│ Verifier  │                                      │
│  │ (Gen)     │      │ (Disc)    │                                      │
│  └───────────┘      └───────────┘                                      │
│       │                  │                                             │
│       ▼                  ▼                                             │
│   [research.md]      [research.md']                                    │
│                                                                        │
│  5. IMPLEMENT        6. VALIDATE                                       │
│  ┌───────────┐      ┌───────────┐                                      │
│  │ Coder     │─────►│ Verifier  │                                      │
│  │ (Gen)     │      │ (Disc)    │                                      │
│  └───────────┘      └───────────┘                                      │
│       │                  │                                             │
│       ▼                  ▼                                             │
│   [code.ts]        [code+tests]                                        │
└────────────────────────────────────────────────────────────────────────┘
```

### Detailed Flow Breakdown

#### 1. Plan Implementation (Generator)
*   **Goal**: Define objective & identify "Unknowns".
*   **Action**: Use **initial research (text and visualization)** to understand patterns, dependencies, and constraints to inform the plan.
*   **Output**: `plan.md` with research questions.

#### 2. Verify Plan (Discriminator)
*   **Goal**: Critique the plan for gaps, assumptions, and clarity.
*   **Action**: Adversarial review and **fact check using initial research**.
*   **Output**: Approved `plan.md` or feedback loop.

#### 3. Create Research (Generator)
*   **Goal**: **Gather more context for implementation** and answer the plan's questions.
*   **Action**: Trace flows (LSP), find patterns (GitHub), validate hints.
*   **Output**: `research.md` (The "RDD Data").

#### 4. Validate Research (Discriminator)
*   **Goal**: Ensure research is sufficient and evidence-backed.
*   **Action**: Verify that all "Unknowns" from the plan are addressed with concrete evidence (line numbers, file paths).
*   **Output**: Approved `research.md` or request for more info.

#### 5. Implement Plan (Generator)
*   **Goal**: Execute the plan using **plan + research docs with clear context and instructions fact based**.
*   **Input**: `plan.md` + `research.md` (RDD Data).
*   **Output**: Code changes + Tests.

#### 6. Validate Implementation (Discriminator)
*   **Goal**: Verify correctness against the Plan, Research, and Rules.
*   **Action**: Run tests, lint, and perform a logic check against the `research.md` findings.
*   **Output**: "Done" signal or fix request.

### The Adversarial Zero-Sum Game
Similar to a GAN, the **Verifier (Discriminator)** tries to find flaws in the **Generator's** output.
*   **Generator's Goal**: Produce output so good the Verifier cannot find faults.
*   **Verifier's Goal**: Find any discrepancy between the output and the "Truth" (Codebase/Context).
*   This tension forces quality up without manual user intervention at every micro-step.

### Cross-Model Validation
If the Verifier is the same model as the Generator (e.g., both are GPT-4o or Claude 3.5), they might share the same blind spots. **Cross-Model Validation** (using a different model for the Verifier) would make this bulletproof. By ensuring different models check each other, we eliminate shared biases and drastically reduce the probability of undetected errors.

> **Note**: The verification step is model-agnostic. You can plug in a smaller, faster model for simple checks or a more reasoning-heavy model (like o1) for deep architectural verification.

### RDD Concrete Example
```text
┌─────────────────────────────────────────────────────────────────────────────────┐
│                       RDD PIPELINE INSTANTIATION                                │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│   A₀ = PRE-RES           A₁ = PLAN              A₂ = RESEARCH                   │
│   ┌─────────────┐        ┌─────────────┐        ┌─────────────┐                 │
│   │ C₀ = {      │        │ C₁ = {      │        │ C₂ = {      │                 │
│   │   goal      │        │   goal,     │        │   plan.md   │                 │
│   │ }           │        │   init_ctx  │        │ }           │                 │
│   └──────┬──────┘        └──────┬──────┘        └──────┬──────┘                 │
│          ▼                      ▼                      ▼                        │
│      [init-ctx]             [plan.md]            [research.md]                  │
│          │                      │                      │                        │
│          ▼                      ▼                      ▼                        │
│                          V₁ = PLAN_VERIFY       V₂ = RESEARCH_VERIFY            │
│                          ┌─────────────┐        ┌─────────────┐                 │
│                          │ Cv₁ = {     │        │ Cv₂ = {     │                 │
│                          │  plan,      │        │ research,   │                 │
│                          │  init_ctx   │        │ sources     │                 │
│                          │ }           │        │ }           │                 │
│                          └──────┬──────┘        └──────┬──────┘                 │
│                                 ▼                      ▼                        │
│                             [plan.md']           [research.md']                 │
│                                                        │                        │
│                                                        ▼                        │
│                                                 A₃ = IMPLEMENT                  │
│                                                 ┌─────────────┐                 │
│                                                 │ C₃ = {      │                 │
│                                                 │  code,      │                 │
│                                                 │  plan, res  │                 │
│                                                 │ }           │                 │
│                                                 └──────┬──────┘                 │
│                                                        ▼                        │
│                                                    [code.ts]                    │
│                                                        │                        │
│                                                        ▼                        │
│                                                 V₃ = CODE_VERIFY                │
│                                                 ┌─────────────┐                 │
│                                                 │ Cv₃ = {     │                 │
│                                                 │  code,      │                 │
│                                                 │  plan, res  │                 │
│                                                 │ }           │                 │
│                                                 └──────┬──────┘                 │
│                                                        ▼                        │
│                                                    [code.ts']                   │
│                                                                                 │
│   VALIDATION CRITERIA (Adversarial):                                            │
│   • V₁: Are there logical gaps? Are unknowns identified? (Checks vs Init Ctx)   │
│   • V₂: Is every claim backed by a file reference?                              │
│   • V₃: Does it pass tests? Does it match the research?                         │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### The Key Principles Validated

| Principle | Document Reference |
|-----------|--------------------|
| Separate sessions | "Each flow (Plan, Research, Implement) is executed by a separate agent or session" |
| Minimal context per action | "The context window should contain the minimal context necessary" |
| Output bridges actions | `[plan.md]` → `[research.md]` → `[code + tests]` |
| Adversarial Checks | "Verifier (Discriminator) tries to find flaws in the Generator's output" |

---

## Part 3: The Octocode Engine

**Octocode is the Research Engine** that powers this methodology. It bridges the gap between what you know (local codebase) and what you need to know (external knowledge).

### Context Pillars
A robust context is built on four pillars:

#### 1. Static Context (The "Knowns")
Immutable or slowly changing sources of truth. **(Powered by `local research tools`)**
*   **Source**: The actual code on disk, definitions, and call graphs.

#### 2. Dynamic Context (The "Unknowns")
External or rapidly changing information. **(Powered by `external research tools`)**
*   **Source**: Remote repos, package internals, and code history.

#### 4. RDD Data (The "Session State")
The artifacts generated during the RDD process itself.
*   **Sources**: research docs heloing coding agents to perform better

### Context Creation
**Octocode is the research initiator.** It proactively aggregates context from three primary sources to build the RDD map:

*   **Known Stuff**: Structured knowledge retrieved from Vector DBs, Knowledge Graphs (KG), and established facts.
*   **Dynamic Research**: Real-time exploration using Octocode tools to traverse code, flows, and external packages.
*   **Existing Context**: Documentation describing the organization, business features, and repositories.

### The Research Engine Architecture
```
+---------------------------------------------------------------+
|                   OCTOCODE RESEARCH ENGINE                    |
+---------------------------------------------------------------+
|                                                               |
|  +------------------+        +------------------------+       |
|  |   LOCAL TOOLS    |        |    EXTERNAL TOOLS      |       |
|  |  (octocode-local)|        |  (octocode-external)   |       |
|  +------------------+        +------------------------+       |
|           |                              |                    |
|           +-------------+----------------+                    |
|                         v                                     |
|           +----------------------------------+                |
|           |   CONTEXT VALIDATION (Hints)     |                |
|           +----------------------------------+                |
+---------------------------------------------------------------+
```

---

## Part 4: The Science & Mechanics

### Core Principle: Minimal Context, Maximum Quality
> **"The context window should contain the minimal context necessary for the agent to implement the task with maximum quality."**

Octocode explicitly targets the **right context** while preserving clear **reasoning steps** with **minimal context pollution**. By utilizing **chained actions**, where the output of one discrete step becomes the input for the next, it ensures the highest, most effective outputs for any purpose of context creation.

Each flow (Plan, Research, Implement) is executed by a **separate agent or session** to adhere to this principle.

### The Equation
$$Quality = \frac{Relevant\ Context}{Context\ Noise} \times Validation \times \epsilon$$

### The Science of Reasoning & Prompting

Octocode connects the tools mechanism using smart prompting which leverages thinking and reasoning, helping the AI agent to navigate through results using hints for smart decisions.

The Octocode manifest highlights a critical shift in how we interact with AI: moving from pattern matching to structured reasoning. To understand why this is necessary, we must look at how AI "thinks" and how a high-quality prompt acts as the catalyst for that thought process.

#### 1. What is AI Reasoning?
In the context of Large Language Models (LLMs), reasoning is the transition from **System 1** (intuitive, fast, pattern-based) to **System 2** (deliberate, slow, logical) thinking.

*   **Pattern Matching (The "Guess")**: Standard AI often predicts the next most likely word based on statistical probability. If you ask for a "login function," it gives you a generic one it has seen a thousand times.
*   **Logical Reasoning (The "Research")**: True reasoning involves breaking a complex goal into intermediate steps. It doesn't just guess the answer; it builds a "Chain of Thought" where each step validates the one before it.

#### 2. Why Good Prompts are the "Map"
If reasoning is the engine, the prompt is the steering wheel. In Research Driven Development (RDD), a prompt isn't just a question; it is an instructional framework. Good prompts are essential for three reasons:

**A. Directing the Attention Mechanism**
LLMs use "Attention," which assigns mathematical weights to different parts of your input.
*   **Low Quality (Noise)**: If a prompt is a "mega-context" dump, the AI's attention is diluted. It might prioritize a random comment in a file over the actual logic of the task.
*   **High Quality (Signal)**: Surgical prompts focus the model’s limited "weights" on the specific constraints and evidence (the Static Context) that matter right now.

**B. Enabling "Chain of Thought" (CoT)**
Reasoning requires "thinking space." By prompting an AI to "reason step-by-step" or "identify unknowns first," you are effectively giving the model permission to use its computational power for logic rather than just fluency. This is the "Generator" phase in your GAN-inspired flow.

**C. Preventing "Context Pollution"**
Every word in a prompt carries baggage. A "good prompt" in the Octocode philosophy is one that provides **Minimal Context**. This prevents "ghosting"—where irrelevant information from a previous attempt haunts the current implementation, leading to hallucinations.

#### 3. The Synergy of Context and Reasoning
The RDD equation you defined, $$Quality = \frac{Relevant\ Context}{Context\ Noise} \times Validation$$, explains why prompting is a science:

*   **Context thinking** is the ability of the AI to hold your codebase's rules in its active memory.
*   **Prompt engineering** is the art of loading only the correct rules into that memory.

Without a sharp prompt, the AI has no "map" to navigate the code. It might have the "truth" (the code), but it lacks the "direction" to find the right path through it.

> **Key Takeaway**: A prompt is not a command; it is the initialization of a state. It tells the AI which "persona" to adopt, which "evidence" to weigh, and which "logic gate" to pass through before writing a single line of code.

### Key Mechanisms

#### 1. The Attention Mechanism
**Why Minimal is Better**: LLMs operate on an "attention mechanism," which assigns weights to different parts of the input. When the context is bloated with irrelevant information ("noise"), the model's attention is diluted, leading to hallucinations or missed details.
*   **The RDD Advantage**: By providing **minimal, highly relevant context**, we maximize the agent's effective attention on the task at hand. A focused 2K context window often outperforms a noisy 50K window because the signal-to-noise ratio is higher.
*   **Attention as a Forensic Lens**: Think of attention weights as a forensic tool: *"Which tokens in this 500-line file have the highest attention weights when considering 'error handling' logic?"* This helps ignore boilerplate and focus on the meat of the logic. RDD tools are designed to extract exactly what the attention mechanism needs.

#### 2. The "Lost in the Middle" Phenomenon
Research shows that Transformer models excel at remembering the **beginning and end** of a context window, but attention often **dips in the middle**. This has critical implications:
*   **The Problem**: Critical information buried in the middle of a massive context dump may be effectively invisible to the model.
*   **The RDD Solution**: By keeping context minimal and surgically relevant, we ensure important information stays within the model's "high attention" zones. Pagination and chunking strategies in Octocode tools are designed with this phenomenon in mind.

#### 3. Tokenization & Flattening: Why Structure Gets Lost
Before an LLM can process anything, it splits input into **tokens**—a linear sequence of pieces.

```
JSON:       {"a": {"b": 1}}
Tokens:     { " a " : { " b " : 1 } }
```

**The Flattening Problem**: Code and data structures are inherently **hierarchical** (trees), but Transformers can only consume **sequences**.

```
Tree:                       Flattened Sequence:
house                       [house, kitchen, fridge, bedroom, bed]
 ├─ kitchen
 │   └─ fridge              → Parent/child relationships LOST
 └─ bedroom
     └─ bed
```

*   **Why AST/JSON Often Fails**: Tokenization only sees sequential pieces—it has **no concept of hierarchy** or parent/child relationships. A deeply nested JSON structure becomes a flat token list where structural meaning is implicit at best.
*   **The RDD Approach**: Instead of dumping raw AST/JSON, Octocode uses **semantic extraction** (LSP call hierarchies, definitions) to provide context that explicitly encodes relationships the tokenizer would otherwise lose.

#### 4. Signals & Delimiters: Guiding Attention
Well-placed **delimiters and structural signals** help the model's attention mechanism identify boundaries and relevance:
*   **Tool Delimiters**: Clear markers between tool outputs help the model segment information logically.
*   **Questioning as Attention Focus**: When the LLM formulates a question (e.g., "Where is X defined?"), it primes the attention mechanism to weight subsequent related tokens higher. RDD flows leverage this by structuring research around explicit questions from the plan.

#### 5. Pagination & Chunking for Semantic Coherence
Large outputs are paginated not just for token limits, but for **semantic clarity**:
*   **Chunking Strategy**: Break content at natural semantic boundaries (functions, classes, sections) rather than arbitrary byte limits.
*   **Progressive Disclosure**: Provide summaries first, details on demand—matching how attention works best.

#### 6. The Check-and-Balance Mechanism
**The Reality Check**: A crucial component of this equation is the "check-and-balance" mechanism. This step validates that the gathered context (the map) faithfully represents the physical reality of the environment (the territory).
*   **Function**: It actively verifies assumptions against the actual codebase state (e.g., "Does this file actually exist?", "Is this function actually exported?").
*   **Outcome**: This ensures the agent never plans or implements based on hallucinations or outdated mental models.

### Why Clean Fresh Context Window Matters? (The Science)

**The concept of clean window context is critical for robust implementation.**

$$ ACTION_1 \rightarrow OUTPUT \rightarrow ACTION_2 \rightarrow ... $$

Each action operates with a **fresh context window**, utilizing only the *output* of the previous action as its *input*.

#### Why it matters:

1.  **Eliminates Context Pollution**: Prevents "ghosts" from previous attempts or unrelated files from confusing the model.
2.  **Maximizes Attention**: The model's attention mechanism is fully focused on the immediate task inputs, not diluted by history.
3.  **Enforces Modularity**: By treating steps as isolated functions (`f(plan) -> research`, `f(research) -> code`), we create a debuggable, deterministic pipeline.


### Patterns vs. Anti-Patterns

| Principle | Anti-Pattern | RDD Pattern |
|-----------|--------------|-------------|
| **Focus** | Dump entire codebase | Surgical extraction via LSP |
| **Relevance** | "Just in case" context | Evidence-based inclusion |
| **Freshness** | Stale cached context | Real-time research |
| **Isolation** | Shared mega-context | Per-session minimal context |

---

## References

*   [Recursive Meta-Metacognition: A Hierarchical Model of Self-Evaluation](https://www.researchgate.net/publication/391826471_Recursive_Meta-Metacognition_A_Hierarchical_Model_of_Self-Evaluation)
*   [Generative Adversarial Network](https://en.wikipedia.org/wiki/Generative_adversarial_network)
*   [Chain of Verification](https://github.com/ritun16/chain-of-verification)
*   [Lost in the Middle: How Language Models Use Long Contexts](https://arxiv.org/abs/2307.03172)
*   [Attention Is All You Need](https://arxiv.org/abs/1706.03762)
