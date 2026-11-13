# Cancellation across extension-host RPC

## Goal

Investigate the following scenario: A caller cancels a long extension operation while a result is already in flight.

## Questions

- What happens when cancellation races with a VS Code extension-host RPC response?
- Does cancellation stop execution, suppress delivery, or both?
- Who owns request and resource cleanup in each possible ordering?
