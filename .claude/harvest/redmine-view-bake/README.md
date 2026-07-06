# redmine-view-bake — leg 1 (Redmine ERB), measured 2026-07-06

DATA, not code (fuzzy-recipe-codebook §8c). The mask hex is read
against field_order.ndjson in THIS directory — regenerate both
together, never independently (I-LEGACY-API-FEATURE-GATED).

- corpus: RAILS_CORPUS_SRC=/home/user/redmine (ns=redmine)
- erb files scanned: 506 · views with hits: 240 · (view,model) rows: 342
- E1 median coverage: 0.667 (pre-reg, plan of record: >=0.60 stands, 0.30-0.60 partial, <0.30 KILL)
- E2: askama==bit-walk on 244 rows; jinja witnessed=true
- E3 reuse: 22 class(es) with ratio < 1.0 among >=3-view classes
- classids: null (v1 bakes namespace-locally; redmine-canon mint is a follow-up)
- wide classes (>64 fields): recorded, render-skipped until OGAR #163

Probe: crates/op-codegen-pipeline/tests/render_bake_probe.rs
