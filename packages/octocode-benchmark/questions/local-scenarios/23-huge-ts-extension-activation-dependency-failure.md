# Extension activation dependency failure

## Goal

Investigate the following scenario: A VS Code extension is installed but a command never activates successfully.

## Questions

- Why might an installed VS Code extension fail to activate when its command is invoked?
- How do activation events, dependencies, and host readiness determine the outcome?
- How do cycles, missing dependencies, and extension exceptions differ in their visible effects?
