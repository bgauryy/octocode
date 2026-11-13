# Partial apply and state persistence

## Goal

Investigate the following scenario: A Terraform provider creates a resource but returns diagnostics before apply completes.

## Questions

- What happens when a Terraform provider creates a resource but reports an error before apply completes?
- Which partial results become state, and when are they persisted?
- How do provider errors, unknown values, and interruption affect recovery and the risk of untracked resources?
