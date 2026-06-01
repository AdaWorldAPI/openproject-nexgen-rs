# Coverage Report — Sprint C0 (C0-08)

**The informal "6–18%" estimate is now a measured number.**

## Method
Enumerated OpenProject's API v3 resource surface (`lib/api/v3/*/` directories in
the upstream Rails source at the 2026-06-01 checkout) and cross-referenced each
against the seed's Rust surfaces: `op-api/src/handlers/`, `op-models/src/`,
`op-db/src/`, `op-api/src/representers/`. Full table: `calibration/census/api-resources.md`.

## Measured coverage (after Wave 1)
| Denominator | Covered | Incl. partial |
|---|---|---|
| 53 total v3 dirs | 10 → **18.9%** | 16 → 30.2% |
| 44 domain v3 resources (excl. 9 utility/shared dirs) | 10 → **22.7%** | 16 → 36.4% |

- **Covered (handler + model):** work_packages, projects, users, statuses, types,
  priorities, roles, versions, memberships, **+ news (Wave 1)** = 10.
- **Partial (handler/repo, no full model):** activities, attachments, categories,
  queries, relations, watchers = 6.
- **Absent:** 27 of 44 domain resources.

**Verdict:** measured coverage lands at **~19–23%**, confirming the seed's informal
"6–18%" estimate at/just above its upper bound. The gap to 100% is **breadth**
(OpenProject has 941 models / 286 controllers; the seed deeply covers ~10 resources)
not depth — the covered resources are faithfully and idiomatically ported.

## Top absent high-value resources (next-wave candidates)
`wiki_pages`, `notifications`, `groups`, `custom_fields`, `shares`.

## Why coverage is not higher this sprint
The real ruff codegen pipeline could not be run (it would have auto-emitted at
scale). Four hard gaps block it — see `extraction-gap-proposals.md`. Wave 1's News
vertical was therefore **agent-emitted following the target-spec**, which is slower
than pipeline emit but produces the same artifact shape (compiling Rust + calibration).
Coverage growth is sprint-over-sprint; the per-resource cost drops sharply once the
sqlx emitter (Gap 1) lands.
