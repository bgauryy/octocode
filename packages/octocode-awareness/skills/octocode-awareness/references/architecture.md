# Awareness Architecture

Awareness exposes one routine surface: Context, Work, Message, Memory, and History. The host-bound client and CLI execute the same operation contracts. Operator-only setup and administration remain outside routine model traffic.

The physical SQLite file plus normalized workspace identity define the coordination boundary. Linked Git worktrees may share a store; unrelated clones or databases do not. Host identity, workspace, database, and scope remain fixed across continuations.

Pi owns native lifecycle events. Shell hosts may install hooks. Workspace policy selects exactly one owner per host, preventing duplicate presence, capture, and delivery.

LocalGit stores recoverable file bytes. Its capture lifecycle is host-owned; routine operations only inspect and restore. Git, LocalGit, and the Awareness database are different evidence sources and none proves another actor's intent.
