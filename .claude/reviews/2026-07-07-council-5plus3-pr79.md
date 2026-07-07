# 5+3 hardening council — PR #79 (Railway deploy + hydration), main @ 65ec718

> 5 research savants (schema-fidelity, security-posture, ops/boot,
> migration-lifecycle, canon-alignment) + 3 brutally-honest reviewers
> (correctness, anti-pattern/boundary, overclaim). All read-only; verdicts
> below with dispositions. Fixes landed via `claude/council-hardening-79`.

## Ledger (deduped, by severity)

| # | Finding | Source | Grade | Disposition |
|---|---|---|---|---|
| 1 | `require_authentication: false` hardcoded on a public URL; work_packages/relations/time_entries/queries writes UNGATED for anonymous | S2 (R1/R2 concur) | **P0** | **FIXED** — secure by default; `OP_ALLOW_ANONYMOUS=1` opens demo posture per-instance |
| 2 | Any unvalidated `Authorization: Bearer/Basic <junk>` mapped to user id 1 (seeded sysadmin) → own-row email/password rewrite | S2 | P1 | **FIXED** — presented credentials rejected outright until real token validation exists |
| 3 | Railway healthcheck on `/health` = hardcoded "OK", blind to DB; DB-less deploy stays "healthy" while API 500s | S3 | P1 | **FIXED** — `healthcheckPath = /health/ready` (DB-probing; pool=None → Unhealthy) |
| 4 | `HYDRATE = "1"` committed in railway.toml — the gate defaulted ON; mock data would seed any attached DB | S3, R2 | P1 | **FIXED** — removed from [env]; both demo gates documented as per-instance dashboard vars |
| 5 | `/api/v3` namespace split across two routers; startup panic the day op-api ports `/configuration` | R2 | P1 | **FIXED** — op-api is the single owner; `/configuration` moved into op-api; local nest deleted |
| 6 | 3× bare `ON CONFLICT DO NOTHING` dead (no UNIQUE): member_roles, role_permissions, query_menu_items → silent duplicates | S1 | P1 | **FIXED in 0001** (legal: no real deploy has applied it yet) — UNIQUE added ×3 |
| 7 | Decode-panic traps: `RolePermissionRow` non-Option over nullable cols; `ProjectRow.lft/rgt` i32 over nullable | S1 | P1 | **FIXED in 0001** — NOT NULL (+DEFAULT 0 for lft/rgt), divergences documented |
| 8 | sqlx checksum trap undocumented + contradicted ("safe to re-run" in 0001; "no migrations needed" in DEPLOYMENT.md) → first post-deploy edit bricks all instances | S4 | P1 | **FIXED** — APPEND-ONLY banner in 0001, §Schema evolution in DEPLOY-RAILWAY.md, DEPLOYMENT.md superseded-note, pool.rs doc corrected |
| 9 | reality-check.sh `runuser postgres` unguarded | S3 | P1 | **FIXED** — explicit guard + actionable error |
| 10 | schema ↔ op-generated field drift (`responsible_id bigint` vs `responsible: Option<u64>`; disjoint field sets) — detonates at W6 | S5 | P1 (dormant) | **RECORDED** here + plan note; reconciliation owed when W6 wires more consumers |
| 11 | Dead divergent handlers (api_root/api_configuration/api_current_user) + live root lost HAL `_links` | R1, R2 | P2 | **FIXED** — deleted; `_links` + real version folded into op-api's root |
| 12 | `base_url = http://0.0.0.0:8080` (bind addr) — wrong HAL links once pagination wires | R1, R2 | P2 | **FIXED** — `PUBLIC_URL` / `RAILWAY_PUBLIC_DOMAIN` preferred |
| 13 | "Proven end-to-end" overclaims (container/Railway leg never executed); "125/125" session-ephemeral; idempotency never exercised twice | R3 | P2 | **FIXED** — docs reworded (§Evidence status); double-apply pass added to reality-check.sh (idempotency now proven by execution) |
| 14 | Seed self-labels "0002 migration" | S4, R3 | P2 | **FIXED** — header rewritten as seed-batch-not-migration |
| 15 | Table-count drift (22/23/24 across docs) | S4, R1, R3 | P2 | **FIXED** — 23 everywhere |
| 16 | Password hashing = DefaultHasher (non-crypto) | S2 | P1 (latent) | **DEFERRED** — replace with argon2 before any real auth path goes live (owed) |
| 17 | `/metrics`, `/health/full` public; CORS `Any` | S2 | P2 | **DEFERRED** — revisit alongside real auth (CORS must tighten before cookie auth) |
| 18 | reality-check.sh applies SQL via psql, not sqlx::migrate! (proxy can stay green where real boot fails checksum) | R2, R3 | P2 | **DOCUMENTED** in the script header; binary-boot leg covered by the KEEP=1 flow |
| 19 | No CI exercises the release build / harness | S3, R3 | P2 | **DEFERRED** — no CI infra in this repo yet; note stands here |

## Verified-clean (feared, disproven with evidence)
- Seed atomicity: one implicit transaction (simple query protocol) — S3/S4, sqlx source read.
- Sequence collisions: 10/10 seeded serial tables `setval`'d — S1/S4/R2 independently.
- Multi-replica migration race: sqlx takes `pg_advisory_lock` — S3, vendored source.
- Docker build: release compile verified live; image tag exists; git deps outside op-server's closure; `.dockerignore` present — S3.
- No credentials in seed; no secrets in railway.toml/Dockerfile — S2.
- Route clash today: disjoint sets, construction-tested — R1/R2 (now moot; single owner).

## Post-fix verification (this branch)
- Secure default: `GET/DELETE /api/v3/work_packages` → **401** without `OP_ALLOW_ANONYMOUS`; `Bearer junk` → **401 in both postures**; static root + configuration public (200); `/health/ready` 200 with DB, Unhealthy path on pool=None.
- Demo posture (`OP_ALLOW_ANONYMOUS=1 HYDRATE=1`): board serves, total=40.
- reality-check.sh: green including the new **double-apply** (counts stable 3/5/40/8/5/0).
- op-db 26 / op-api 3 / op-server 31 tests green.
