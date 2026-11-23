//! The generator of `crates/op-desk/src/generated/`, shared by
//! `examples/generate.rs` (writes the files) and `tests/regenerate.rs`
//! (compares them).
//!
//! It runs `rails-compiler` the way `rails-compile <app.toml>` does for
//! zammad.toml, with two inputs replaced because the compiler cannot read
//! OpenProject's schema:
//!
//! 1. `rails_compiler::config::Loaded::read` loads `openproject.toml`.
//! 2. The compiler's own harvest finds every slice model, its table
//!    (`self.table_name` or the naming convention), ancestors, source files and
//!    `store` attributes (`Harvest::collect`, `Assembler::expand`,
//!    `semantic::table_name`); `ogar_from_ruff::lift_model` gives the OGAR view.
//! 3. The schema: `rails_compiler::schema::Schema::read` yields no table for
//!    OpenProject (its schema is `db/structure.sql`, and the baseline
//!    `db/migrate/tables/*.rb` use a form it does not walk). So
//!    `op_desk::structure` reads `db/structure.sql` into the compiler's own
//!    `Schema` / `TableDef` / `ColumnDef`, with Rails type names.
//! 4. `rails_compiler::emit::write` emits the crate files, into a temporary
//!    directory.
//! 5. Post-mapping: the emitter writes `date` as `DateTime` and `float` as
//!    `Decimal` (zammad-rs's mapping). Each such column line is rewritten with
//!    `op_desk::types::polyfill_type`, and the emitter's zammad-specific
//!    "Regenerate:" header line is replaced.
//!
//! Callbacks and associations are not compiled: every chain is empty.

pub mod scopes;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use rails_compiler::config::Loaded;
use rails_compiler::harvest::{Harvest, UnitKind};
use rails_compiler::schema::{ColumnDef, Default as SchemaDefault, Schema, TableDef};
use rails_compiler::semantic::{table_name, Assembler};
use rails_compiler::{module_name, Compilation, CompiledModel, CorpusCoverage, OgarView};

pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

const ZAMMAD_REGEN: &str =
    "// Regenerate: ZAMMAD_SRC=<zammad checkout> cargo run -p rails-compiler -- zammad.toml\n";
const OP_REGEN: &str = "// Regenerate: OPENPROJECT_SRC=<openproject checkout> cargo run -p op-desk --example generate\n// (openproject.toml; rails-compiler's emitter, then op_desk::types post-mapping for date/float)\n";

/// OpenProject's own path for the schema the capture copies.
const OP_STRUCTURE: &str = "db/structure.sql";

/// The schema file named by `[op_desk].structure`, relative to openproject.toml.
pub fn structure_path(cfg_path: &Path) -> Result<String, String> {
    let text = std::fs::read_to_string(cfg_path).map_err(|e| e.to_string())?;
    let v: toml::Value = toml::from_str(&text).map_err(|e| e.to_string())?;
    v.get("op_desk")
        .and_then(|o| o.get("structure"))
        .and_then(toml::Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| "openproject.toml has no [op_desk].structure".into())
}

fn to_column(c: &op_desk::structure::SqlColumn, rel: &str) -> ColumnDef {
    use op_desk::structure::{rails_default, rails_type, RailsDefault};
    let (ty, limit) = rails_type(&c.sql_type);
    ColumnDef {
        name: c.name.clone(),
        ty,
        null: !c.not_null,
        default: match rails_default(c.default.as_deref()) {
            RailsDefault::None => SchemaDefault::None,
            RailsDefault::Int(i) => SchemaDefault::Int(i),
            RailsDefault::Str(s) => SchemaDefault::Str(s),
            RailsDefault::Bool(b) => SchemaDefault::Bool(b),
            RailsDefault::Expr(e) => SchemaDefault::Expr(e),
        },
        limit,
        provenance: format!("{rel}:{}", c.line),
    }
}

/// File name → contents of `crates/op-desk/src/generated/`.
pub fn generate(cfg_path: &Path) -> Result<BTreeMap<String, String>, String> {
    let cfg = Loaded::read(cfg_path)?;
    let app = &cfg.config.app;
    let structure_rel = structure_path(cfg_path)?;
    let base = cfg_path.parent().unwrap_or_else(|| Path::new("."));
    let sql = std::fs::read_to_string(base.join(&structure_rel))
        .map_err(|e| format!("{structure_rel}: {e}"))?;
    // Provenance names the file in OpenProject the capture is a copy of.
    let structure_rel = OP_STRUCTURE;
    let dump = op_desk::structure::parse(&sql);

    let h = Harvest::collect(&cfg.source, &app.autoload_roots, &app.namespace);
    let fw =
        rails_polyfill::rules::FrameworkDefaults::for_version(app.rails_load_defaults.as_deref());
    let mut asm = Assembler::new(&h, fw, app.framework_roots.clone());
    let is_model = |n: &str| {
        h.units
            .get(n)
            .is_some_and(|us| us.iter().any(|u| u.kind == UnitKind::Class))
    };
    let (graph, _) = ruff_ruby_spo::extract_app_with_schema(&cfg.source, &app.namespace);

    let mut schema = Schema::default();
    let mut models = Vec::new();
    let mut stores = BTreeMap::new();
    for m in &cfg.config.slice.models {
        if !h.units.contains_key(m) {
            return Err(format!("slice model {m} not found in the harvest"));
        }
        let table = asm
            .explicit_table_name(m)
            .unwrap_or_else(|| table_name(m, &is_model, &h.irregulars));
        if !op_desk::is_slice_table(&table) {
            return Err(format!("{m}: table {table} is not a journey or read table"));
        }
        let t = dump
            .get(&table)
            .ok_or_else(|| format!("{m}: table {table} not found in {structure_rel}"))?;
        let provenance = format!("{structure_rel}:{}", t.line);
        schema.tables.insert(
            table.clone(),
            TableDef {
                name: table.clone(),
                columns: t
                    .columns
                    .iter()
                    .map(|c| to_column(c, structure_rel))
                    .collect(),
                provenance: provenance.clone(),
            },
        );
        let e = asm.expand(m);
        let ogar =
            graph
                .models
                .iter()
                .find(|x| &x.name == m)
                .map_or_else(OgarView::default, |rm| {
                    let c = ogar_from_ruff::lift_model(rm);
                    OgarView {
                        lifted: true,
                        name: c.name.clone(),
                        parent: c.parent.clone(),
                        associations: c.associations.len(),
                        callbacks: c.callbacks.len(),
                        validations: c.validations.len(),
                        mixins: c.mixins.len(),
                    }
                });
        let source_files = h
            .units
            .get(m)
            .map(|us| {
                us.iter()
                    .filter_map(|u| h.file(&u.file).map(|f| f.rel.clone()))
                    .collect()
            })
            .unwrap_or_default();
        stores.insert(m.clone(), e.stores.clone());
        models.push(CompiledModel {
            name: m.clone(),
            module: module_name(m),
            table,
            table_provenance: provenance,
            source_files,
            ancestors: e.ancestors.clone(),
            chain: Vec::new(),
            ogar,
            lowered_methods: BTreeMap::new(),
            residual_methods: BTreeMap::new(),
            host_calls: Default::default(),
            aliases_used: Default::default(),
            dynamic_attr: Default::default(),
            folded: Default::default(),
            unresolved: Vec::new(),
            stores: e.stores.clone(),
            navigable: Vec::new(),
            not_navigable: Vec::new(),
            code: Default::default(),
            conds: Vec::new(),
        });
    }
    let compilation = Compilation {
        app: app.name.clone(),
        models,
        relations: Vec::new(),
        schema_cross_check: Vec::new(),
        schema_unparsed: Vec::new(),
        corpus: CorpusCoverage::default(),
        harvest_parse_failures: Vec::new(),
        schema,
    };

    let tmp = std::env::temp_dir().join(format!("op-desk-gen-{}-{}", std::process::id(), unique()));
    let _ = std::fs::remove_dir_all(&tmp);
    rails_compiler::emit::write(&compilation, &stores, &tmp).map_err(|e| format!("emit: {e}"))?;
    let mut out = BTreeMap::new();
    for entry in std::fs::read_dir(tmp.join("src")).map_err(|e| e.to_string())? {
        let path = entry.map_err(|e| e.to_string())?.path();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string();
        let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        let file = if name == "lib.rs" {
            "mod.rs".to_string()
        } else {
            name.clone()
        };
        let model = compilation
            .models
            .iter()
            .find(|m| format!("{}.rs", m.module) == name);
        let text = match model {
            Some(m) => post_map(
                &text,
                &compilation.schema.tables[&m.table],
                &stores[&m.name],
            )?,
            None if name == "lib.rs" => raw_keyword_modules(&text, &compilation),
            None => text,
        };
        if !text.contains(ZAMMAD_REGEN) {
            return Err(format!(
                "{name}: the emitter's header changed; update the generator"
            ));
        }
        out.insert(file, text.replacen(ZAMMAD_REGEN, OP_REGEN, 1));
    }
    let _ = std::fs::remove_dir_all(&tmp);

    // The default scopes of the slice models (op_desk::scopes).
    let inputs: Vec<scopes::ModelInput> = compilation
        .models
        .iter()
        .map(|m| {
            let mut chain = vec![m.name.clone()];
            while let Some(sup) = chain.last().and_then(|c| asm.superclass(c)) {
                if chain.contains(&sup) || !h.units.contains_key(&sup) {
                    break;
                }
                chain.push(sup);
            }
            scopes::ModelInput {
                model: m.name.clone(),
                table: m.table.clone(),
                chain,
                columns: compilation.schema.tables[&m.table]
                    .columns
                    .iter()
                    .map(|c| c.name.clone())
                    .collect(),
            }
        })
        .collect();
    let lowered = scopes::lower(&h, &inputs);
    let header = format!("// @generated by op-desk's generator — do not edit.\n{OP_REGEN}");
    out.insert("default_scopes.rs".into(), scopes::emit(&lowered, &header));
    let m = out.get_mut("mod.rs").ok_or("no mod.rs emitted")?;
    let anchor = "pub mod relations;\n";
    if !m.contains(anchor) {
        return Err("mod.rs: `pub mod relations;` not found; update the generator".into());
    }
    *m = m.replacen(anchor, &format!("{anchor}pub mod default_scopes;\n"), 1);
    Ok(out)
}

/// The emitter declares a module named after `Type` as `pub mod type;`, which
/// is a keyword. Such a module is declared as a raw identifier
/// (`pub mod r#type;`, still `type.rs`).
fn raw_keyword_modules(text: &str, c: &Compilation) -> String {
    const KEYWORDS: &[&str] = &[
        "type", "match", "move", "ref", "self", "super", "crate", "mod", "use", "impl", "fn",
        "struct", "enum", "trait", "where", "loop", "box",
    ];
    let mut out = text.to_string();
    for m in &c.models {
        if KEYWORDS.contains(&m.module.as_str()) {
            out = out
                .replace(
                    &format!("pub mod {};", m.module),
                    &format!("pub mod r#{};", m.module),
                )
                .replace(
                    &format!("&{}::MODEL", m.module),
                    &format!("&r#{}::MODEL", m.module),
                );
        }
    }
    out
}

fn unique() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos())
}

/// Rewrites the `ty:` of every column whose polyfill type differs from what
/// the emitter wrote (the `date` and `float` columns).
fn post_map(text: &str, table: &TableDef, stores: &[String]) -> Result<String, String> {
    let mut out = text.to_string();
    for c in &table.columns {
        let store = stores.contains(&c.name);
        let want = op_desk::types::variant(op_desk::types::polyfill_type(&c.ty, store));
        let prefix = format!("rp::Column {{ name: {:?}, ty: rp::ColumnType::", c.name);
        let at = out.find(&prefix).ok_or_else(|| {
            format!(
                "{}.{}: column line not found in the emitted file",
                table.name, c.name
            )
        })?;
        let start = at + prefix.len();
        let end = start + out[start..].find(',').ok_or("malformed column line")?;
        if &out[start..end] != want {
            out.replace_range(start..end, want);
        }
    }
    Ok(out)
}
