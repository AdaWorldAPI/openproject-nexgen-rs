# Sprint C1 Gap 1 — work-in-progress snapshot

This directory archives the live state of the Gap-1 work in
`/home/user/ruff-clone/` (workbench for `AdaWorldAPI/ruff`), because that
workbench is **signing-blocked**: the session's commit-signing server only
accepts repositories with local-proxy origins (the three MCP-scoped repos);
public-repo clones cannot be locally committed.

The work-in-progress therefore lives in the working tree of `ruff-clone/` and
is mirrored here at session-end snapshots:

- `WIP.tar.gz`     — full tarball of the new files (sqlx_emit/, goldens,
                     tests, examples, docs).
- `CURRENT.patch`  — `git diff main` against the cloned upstream (covers any
                     edits to existing tracked files; right now this is empty
                     because Wave 1 only adds new files).

Wave 1 is in flight at the time of this snapshot — 5 of 8 fanout agents have
landed (A1 list_for_tenant, A5 SQLX-TARGET.md, A6 example TOML, A8 spec test;
plus A7 sqlx_emit_test scaffolding). The remaining 3 (A2 detail, A3
soft_delete, A4 toggle) are still working in the background and will be added
in the next snapshot.

After Wave 2 (orchestrator wiring of target.rs + dispatch in codegen/mod.rs)
and Wave 3 (cargo check + cargo test), the final deliverable will be a single
`sprint-c1-gap1.patch` ready for the user to apply on top of
`AdaWorldAPI/ruff@main` and push.

The ephemeral .sprint/ logs from ruff-clone are not archived (orchestration
noise; recreatable).
