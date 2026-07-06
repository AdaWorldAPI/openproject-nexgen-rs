# op-representer-bake — leg 2 (OP representers), measured 2026-07-06

DATA, not code (fuzzy-recipe-codebook §8c). The mask hex is read
against field_order.ndjson in THIS directory — regenerate both
together, never independently (I-LEGACY-API-FEATURE-GATED).

- corpus: OP_CORPUS_SRC=/tmp/op-corpus (ns=openproject)
- representer files with declarations: 104 · mapped rows: 52 · unmapped files: 52
- L2-E1 median coverage: 0.429 (pre-reg: >=0.60 stands, 0.30-0.60 partial, <0.30 KILL)
- L2-E2: askama==bit-walk on 36 rows (0 wide); jinja witnessed=true
- CONV-1: jaccard=0.464 vs Redmine-Issue (leg 1 artifact, C4-renamed); renames_applied=2 (partial (disjoint census is the deliverable))
- classids: null (v1 bakes namespace-locally)

Probe: crates/op-codegen-pipeline/tests/render_bake_leg2_probe.rs
Note: leg 2, pre-reg L2-E1/L2-E2/CONV-1; wide leg wired via OGAR #163.
