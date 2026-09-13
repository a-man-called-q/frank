# Team roles and taskboard

Frank uses the taskboard as the durable hand-off protocol between agents. A
provider session is not a mailbox: an agent records work in the task card and
its activity feed, and another agent discovers the next action by reading that
same record.

## Ownership model

### Roles are the source of truth

A `RoleView` is a complete worker template, not only a label. It contains the
provider/model choice, pack and level, instructions, policy, budget, and avatar.
An agent has one primary `role_id`. The role revision copied into the agent is
the revision that its current provider session started with.

Role changes are applied at an idle boundary (or when a role is explicitly set
on an idle/offline agent). A running agent therefore cannot change behavior in
the middle of a task. A role cannot be archived while an agent or task still
references it.

`CreateAgent` accepts a role and materializes its template. The legacy
role-less form remains readable during migration; new clients should always
send a role. `CreateRole`, `UpdateRole`, `ArchiveRole`, and `SetAgentRole` are
the server commands for managing the team template catalog.

### Tasks target a role, not a person

`TaskSpec.required_role_id` is the queue key. An optional explicit
`assigned_agent` is a manual override and must belong to that role. Otherwise
the daemon chooses an eligible member when the task is ready:

1. the mission is not final;
2. every dependency is `Done`;
3. the task is not already claimed or running;
4. the agent is unarchived and `Idle` or `Offline`;
5. the agent's primary role matches the task role.

Eligible agents are sorted by `last_claimed_at` (oldest first), then by stable
agent id. This gives least-recently-claimed fairness without requiring a
provider to be alive while it is waiting. The claim transaction records the
agent, `claimed_at`, `claim_source`, and a `Claimed` feed entry atomically.

The sources are `automatic`, `manual`, and `reclaim`. Manual assignment is
still checked against the same role, availability, dependency, and mission
guards; it is not a way to bypass queue safety. A running task must be stopped
before its claim can be released.

## Task activity feed

`Snapshot.task_feed` is a flat, newest-first timeline. Comments and system
events use the same `TaskFeedEntry` shape, so a hand-off is self-contained and
auditable:

- creation, status changes, claims, releases, and dependency locks/unlocks;
- human/agent comments with optional artifact ids;
- future attachment-specific events without introducing another mailbox.

`AddTaskComment` validates the body and verifies that every linked artifact is
from the same mission and is either task-owned or explicitly unscoped. The
feed is bounded to the newest 2,048 entries in the live snapshot. SQLite keeps
the projected rows and task/artifact links; the event and audit logs remain the
source for older history.

Direct agent-to-agent `SendMessage` delivery is rejected. An agent may message
the supervisor, but cross-agent coordination belongs in the task feed. This
keeps the task card, not a provider process, as the source of truth.

## Dependencies and lifecycle

Dependencies are checked both when a task is claimed and when it is moved to
`Ready` or `Running`. A task whose dependency is unfinished cannot be claimed.
When a dependency becomes `Done`, dependents move from `Blocked` to `Ready`
and receive a `DependencyUnlocked` entry. If an upstream task is made
unavailable again, a ready dependent is marked `Blocked` with the corresponding
feed event.

The normal flow is:

```text
Backlog -> Ready -> Running -> Review -> Done
                    |           |
                    +-> Blocked <-+
```

`TaskAccept` completes a manual task immediately, or creates the durable
commit operation for a worktree-backed task. Completion also reevaluates and
unlocks dependents.

## Remote desktop contract

The desktop uses `FrankGateway`; production authentication selects the HTTP
gateway and fixture data is restricted to demo/tests. The HTTP gateway reads
roles, role revisions, claims, and activity from `/v2/snapshot`, and sends
`claim_task`, `release_task`, and `add_task_comment` command envelopes through
the authenticated command endpoint. It also consumes the authenticated,
cursor-based `/v2/events` WebSocket and refreshes the snapshot when another
client changes the board. A short polling interval remains as a recovery path
for reconnect gaps; neither transport creates a provider-to-provider channel.

The Team surface shows the role template/revision attached to each member and
the taskboard inspector exposes the role-aware claim/release controls plus the
shared activity composer. Organization remains a separate scope and is not a
dependency of this first Team/Taskboard slice.

## Invariants to preserve

- Exactly one primary role per agent; role templates win over per-agent copies.
- Provider processes start only after a task claim; idle agents consume no
  provider slot.
- A claim, agent fairness timestamp, and feed entry commit together.
- A task cannot run or be claimed before its dependencies are done.
- Every hand-off has a durable activity record; no direct agent-to-agent
  mailbox is required for correctness.
- Artifact retention must honor task feed links while the task remains alive.
