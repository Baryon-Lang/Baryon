# 0002. The roadmap is the source of truth for project management

Status: Accepted
Date: 2026-11-06

## Context

Humans and AI agents work on the project in parallel, often in separate sessions that share no
memory. Without one shared record, two contributors can start the same work, finished work goes
unrecorded, and ideas raised in passing are lost. Issue trackers and chat threads live outside the
repository, so an agent cloning the code does not see them.

## Decision

- `ROADMAP.md` at the repository root lists every phase, work item, open decision and idea, with
  status, owner, dependencies and a link to the pull request.
- Contributors mark an item `In progress` when they start and `Done` in the pull request that
  finishes it. New ideas go in its Ideas table.
- `cargo xtask check-roadmap` validates every row and fails any change that does not update
  `ROADMAP.md`. CI runs it on every pull request.

## Consequences

- The plan and the state of the work live next to the code and change with it, so anyone, human or
  agent, can see what to do next.
- Every pull request touches `ROADMAP.md`, so two open pull requests can conflict on it. Each
  conflict is small and local to one row.
- A claim made in an open pull request is not visible on `main` until it merges, so contributors
  also check open pull requests before starting work.
