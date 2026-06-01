# Calibration — News slice (Sprint C0 Wave 1)

Slice emitted: the OpenProject **News** v3 resource, end-to-end, into the seed
crates following the axum+sqlx+HAL idiom. Agent-emitted per
`target-spec/rust-axum-sqlx.toml` (4 file-disjoint agents + orchestrator wiring).

## Files emitted
| Layer | File | Status |
|---|---|---|
| model | `crates/op-models/src/news.rs` | faithful (schema verified vs `db/migrate/tables/news.rb`) |
| repo | `crates/op-db/src/news.rs` | faithful (`find_by_id`, `list_by_project`; runtime sqlx) |
| representer | `crates/op-api/src/representers/news.rs` | faithful (mirrors `lib/api/v3/news/news_representer.rb`) |
| handler | `crates/op-api/src/handlers/news.rs` | `list_news` + `get_news` faithful; CRUD gapped |
| routes | `crates/op-api/src/routes.rs` | `GET /api/v3/news/:id`, `GET /api/v3/projects/:id/news` |

## The five calibrate.rs invariants (per skill)
1. **unmapped-model** — 0 violations. `News` referenced by the handler/repo;
   `project`/`author` association refs are emitted as HAL `_links`. No silent drop.
2. **template-context-mismatch** — N/A. OpenProject is API/HAL-first (Angular SPA);
   no server templates in this slice. (This itself is the ERB-vs-jinja gap, see
   `extraction-gap-proposals.md`.)
3. **form-field-gap** — N/A for the read slice. `CreateNewsDto`/`UpdateNewsDto`
   exist on the model but no write handler is activated (see extractor-gap below).
4. **output-kind-mismatch** — 0 violations. `ajax_json` kind → `HalResponse(...)`
   JSON return; matches the `projects.rs` idiom.
5. **extractor-gap** — 3, all explicit (never `todo!()`):
   - **CRUD** (create/update/delete): `op-db` `NewsRepository` exposes no write
     surface (`crates/op-db/src/news.rs`); a repository-backed mutation cannot be
     emitted without fabricating a method. Resolve: grow the repo write surface
     (mirror `ProjectRepository::{create,update,delete}`), then emit the handlers.
   - **global `GET /api/v3/news`** (project-filtered list): repo has no `list_all`;
     only `list_by_project`. Resolve: add `list_all` + filter parsing.
   - **authz `view_news`**: `list_news`/`get_news` require authentication
     (`AuthenticatedUser`) but do NOT enforce the Rails `before_action` project
     permission `view_news`. This MATCHES the seed's own `projects.rs::get_project`
     (which also ignores `_user`), so it is consistent with the seed — but it is a
     real authorization gap vs OpenProject. Resolve: thread a project-permission
     check through the handler (a cross-cutting concern absent from the seed).

## Verification
- `cargo check --workspace`: **green** (0 errors).
- News tests: **10/10** (`op-models` 6, `op-api` representer 4).
