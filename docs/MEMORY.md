# Memory & Agent Coordination

Three agents work on this repo: **Claude**, **Codex**, and **WorkBuddy**.
This file is how they stay in sync.

## Current state: the repo IS the memory layer

A Memory Hub project for DeskPet **does not exist yet**. As of the last
attempt, `bootstrap`, `start_turn`, `update_state` and `finish_turn` all return
`PROJECT_NOT_FOUND` for project id `DeskPet`, and the server exposes no
create/init endpoint — the project has to be created on the Memory Hub side
first.

Until that happens, cross-agent memory lives in git:

| File | Holds | Changes |
|---|---|---|
| `docs/HANDOFF.md` | Locked decisions, known gaps, licensing landmines | Rarely |
| `docs/STATE.md` | What is in progress, what is next, open questions | Every session |
| `docs/ARCHITECTURE.md` | Why the system is shaped this way | Rarely |
| `docs/PETPACK_SPEC.md` | The pack format contract | Frozen at v0.1.0 |

**Start every session by reading `docs/STATE.md` and `docs/HANDOFF.md`.**
**End every session by updating `docs/STATE.md` in the same commit as your code.**

Git gives us most of what Memory Hub would: durable, auditable, revertible, and
directly greppable by any agent with a clone. Its one real weakness is
concurrency — see below.

## Concurrency

Two agents editing `STATE.md` on separate branches will conflict. That is a
feature: the conflict is visible and must be resolved, rather than one agent
silently overwriting the other.

To keep conflicts cheap:

- Keep `STATE.md` entries short and append-oriented.
- One agent owns one task at a time. Claim it in `STATE.md` before starting.
- Never rewrite another agent's entry. Add a new one that supersedes it.

## Once Memory Hub exists

When the project is created, get the **actual `project_id` the API accepts** —
not the display name. Some implementations slugify the name or use a UUID as
the real key. If Claude writes to `DeskPet` and Codex writes to `deskpet`,
neither errors and each sees half the history. That failure mode is very hard
to diagnose.

Then, for every agent:

```
project_id = "DeskPet"        # exact string, byte for byte, all agents
agent_id   = "claude" | "codex" | "workbuddy"
```

`agent_id` must differ per agent, otherwise the work log cannot attribute
changes — with three agents touching the same code, unattributable history is
close to useless.

### Optimistic locking

`update_state` requires `expected_version`. On mismatch it returns
`STATE_CONFLICT` and **rolls back the entire write** — neither state nor work
log is persisted.

With three agents this will happen in practice. The correct handling is:

1. Call `get_state` to read the current version.
2. Re-apply your change on top of it.
3. Retry `update_state` with the new `expected_version`.

Do **not** blind-retry with the stale version, and do **not** swallow the
error. An ignored `STATE_CONFLICT` looks like a successful write and silently
loses the update.

This paragraph should be included in any task prompt given to Codex or
WorkBuddy that involves writing state.

### Connecting Codex

Codex CLI reads MCP servers from `~/.codex/config.toml`:

```toml
[mcp_servers.agent-memory]
command = "npx"                    # replace with the real launch command
args = ["-y", "@your/memory-hub-mcp"]

[mcp_servers.agent-memory.env]
MEMORY_HUB_URL = "..."             # MUST match what Claude points at
MEMORY_HUB_TOKEN = "..."
```

If the hub is a remote HTTP service rather than a stdio process, use the
`transport = "http"` + `url` form instead.

The critical part is `MEMORY_HUB_URL`. If one agent points at a local instance
and another at a remote one, sharing is nominal only — both will report
success and see different data.

## Delegation guidance

Good to hand off — clear boundary, testable acceptance:

- Implement `load_active_pack`
- Implement `macos::is_dnd_active`
- Replace `webhook::generate_token` with `OsRng`
- Collapse the duplicated `AppState` in `lib.rs`

Keep in-house — exploratory, verdict depends on real hardware, and the outcome
may change the architecture:

- Getting the transparent `NSPanel` floating correctly on real macOS
- Tuning edge decontamination until cutouts stop looking pasted on

For exploratory work the deliverable is a judgement, not a diff, and the
round-trip cost of delegating usually exceeds the saving.
