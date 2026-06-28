# Octocode Brainstorming

`octocode-brainstorming` turns a fuzzy idea into an evidence-grounded decision brief. It is for "is this worth building?", "has anyone built this?", "what are the angles?", and "should we add this?" moments.

It does not write code or design the final system. It decides whether the idea deserves that next step.

## How it works

The skill reframes the idea into testable claims, then checks the local workspace when relevant, GitHub, packages, and the web for prior art and contrary evidence. It groups signals into crowded, partial, abandoned, contested, or open-space buckets, debates objections against opportunities, and ends with a decision brief plus a handoff when an RFC is warranted.

## Good asks

- "Validate this idea before I build it."
- "Has anyone already solved this?"
- "Brainstorm product directions for this technical area."
- "Find prior art and white space for this feature."
- "Should this become an RFC?"

## What you get

- A reframed problem statement so the search is not trapped by the first wording.
- A surface plan covering local code when relevant, GitHub, packages, and the web.
- Prior art grouped into crowded, partial, abandoned, contested, or open-space signals.
- Evidence-backed objections and opportunity angles.
- A verdict such as worth prototyping, narrow first, park, or do not build.
- A handoff packet for `octocode-rfc-generator` when the idea is ready for a design plan.

## Use another skill when

- The user already knows what to build and wants code work: use `octocode-engineer`.
- The question is technical research rather than idea validation: use `octocode-research`.
- The goal is clear and needs repeated proof loops: use `octocode-loop`.
- The decision is made and needs a proposal: use `octocode-rfc-generator`.

## User value

This skill protects users from building the first plausible idea. It widens the frame, checks reality across multiple surfaces, then compresses the result into a brief that supports a decision.
