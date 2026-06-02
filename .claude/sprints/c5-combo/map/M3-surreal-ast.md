# M3 — SurrealQL: construct & render programmatically (read-only map)

Scope: the (ii) "build on surreal's existing AST" surface, for the executable + DDL facets.
Source root: `/home/user/surrealdb/surrealdb/`. Wave-1, Sprint C5.

## TL;DR — there are TWO ASTs, and a values-only stable contract
1. **`surrealdb/ast`** — arena/`NodeId` AST (the one named in the brief). **No SurrealQL rendering** (only debug `vis`, gated `feature="visualize"`). Self-declared **internal/unstable**.
2. **`surrealdb/core/src/sql/`** — a *second*, owned-tree AST (`SelectStatement`, `RelateStatement`, `DefineTable…`) that **does** render SurrealQL via `ToSql`. But its statement types are mostly `pub(crate)` inside the non-public `core` crate.
3. **`surrealdb-types::ToSql`** — stable-ish (flagged EXPERIMENTAL) renderer for **values only** (`Value`/`Kind`/`RecordId`), not statements.

**Recommendation: do NOT build the emitter on either AST. Emit SurrealQL as string templates, interpolate identifiers/DDL fragments, and pass all data as bound params via the SDK `query(..).bind(..)`. Use `surrealdb-types::{Value, ToSql, kind!}` for the typed-value layer.** Reasoning in §3.

---

## 1. The AST surface

### (a) `surrealdb/ast` — arena AST (named in brief; NOT a renderer)
Arena design: every node lives in a `Library` keyed by typed `NodeId<T>`/`NodeListId<T>`; structs hold *ids*, not children.
- `Node`/`UniqueNode`/`NodeLibrary` traits: `ast/src/types/mod.rs:45,47,133`. `NodeId<T>` via `id!` macro `:14`; `NodeList`/`NodeListId` linked list `:73,82`.
- `Ast<L>` arena ops — `ast/src/types/ast.rs:14`: `push<T:Node>(value)->NodeId<T>` `:21`, `push_list(..)` `:41`, `iter_list(..)` `:62`, `Index<NodeId<T>>` `:82`.
- Statement nodes (all `ast_type!`, fields are `NodeId`/`NodeListId`), `ast/src/lib.rs`:
  - `Select` `:542` (`fields: NodeId<Fields>`, `from: NodeListId<Expr>`, `condition: Option<NodeId<Expr>>`, `group/order/limit/…`).
  - `Create` `:449`, `Update` `:472`, `Delete` `:460`, `Upsert` `:485`, `Relate` `:498`.
  - `DefineTable` `:804`, `DefineField` `:903` (`name/table: NodeId<Expr>`, `ty: Option<NodeId<Type>>`, `permissions`, …).
  - `Expr` (the big enum, `Copy`) `:1437`; `Fields`/`Selector`/`ListSelector` `:431/417/424`; `RecordData` (SET/CONTENT/MERGE/…) `:406`.
- **Rendering:** `vis/mod.rs:1` is `#![cfg(feature="visualize")]` and emits a *debug tree* (`AstVis::to_ast_string` `:26`, writes `Foo::Bar` indented), **not** SurrealQL. No `Display`→SurrealQL anywhere in `ast/`. Confirmed: `ast` has no `ToSql`.

> Build-by-hand `SELECT * FROM work_package WHERE project = $p` on `ast` (sketch — verbose, every leaf is a `push`):
> ```rust
> let mut a = Ast::<Library>::empty();
> // fields = *  → Fields::List([ListSelector::All])
> let all  = a.push(ListSelector::All(Span::empty()));
> let (mut h,mut t)=(None,None); a.push_list(all,&mut h,&mut t);
> let fields = a.push(Fields::List(h.unwrap()));
> // FROM work_package  (a Table/Ident expr)
> let tbl_s  = a.push_set("work_package".to_string());
> let tbl_id = a.push(Ident{ text: tbl_s });
> // … wrap as Expr, push into a from-list …
> // WHERE project = $p  → BinaryExpr{ left: idiom(project), op: Equal, right: Param($p) }
> // … push Param, Place, BinaryExpr, build Select{..}, push as Expr::Select …
> ```
> ~15 `push` calls, no validation that the shape is legal, and **still no text out** (you'd then have to lower `ast`→`core::expr` to execute, or write your own printer). This is the wrong altitude for an emitter.

### (b) `surrealdb/core/src/sql/` — owned-tree AST that DOES render
Conventional `Box`/`Vec` tree; **each statement implements `surrealdb_types::ToSql`**:
- `SelectStatement` `core/src/sql/statements/select.rs:10` — **`pub`**; `ToSql` `:31` emits `SELECT {fields} FROM …`.
- `CreateStatement` `…/create.rs:8` **`pub`**, `ToSql` `:34`.
- `RelateStatement` `…/relate.rs:8` **`pub(crate)`**, `ToSql` `:24` (see §4).
- `UpdateStatement` `…/update.rs:8` `pub(crate)`; `DefineTableStatement` `…/define/table.rs:10` `pub(crate)`; `DefineFieldStatement` `…/define/field.rs:46` `pub(crate)`.
- Exposed only as `pub mod sql` of **core** (`core/src/lib.rs:68`); core is not the stable SDK and most statement structs are `pub(crate)`. So this renderer is **not reachable as a public API**.

---

## 2. `surrealdb-types` — the stable(-ish) typed-value + SQL-emit contract
- `ToSql` — `types/src/sql.rs:31`. Header **"⚠️ EXPERIMENTAL: not stable, may change/remove without major bump"** (`:13-20`).
  - `fn to_sql(&self) -> String` `:33`; `fn to_sql_pretty(&self) -> String` `:40`; `fn fmt_sql(&self, f:&mut String, fmt: SqlFormat)` `:47` (the one required method).
- `SqlFormat` — `:52` `enum { SingleLine, Indented(u8) }`; helpers `increment` `:66`, `write_separator` `:83`; `fmt_sql_comma_separated` `:96`, `fmt_sql_key_value` `:121`.
- `write_sql!(f, fmt, "…{}…{named}", args…)` macro — proc-macro `types/derive/src/write_sql.rs:129` (doc `types/derive/src/lib.rs:234`); compile-time format parse, calls `ToSql::fmt_sql` per placeholder. Re-exported `types/src/lib.rs:22`.
- `SurrealValue` trait — `types/src/traits/surreal_value.rs:29`:
  ```rust
  pub trait SurrealValue {
      fn kind_of() -> Kind;
      fn is_value(value: &Value) -> bool { value.is_kind(&Self::kind_of()) }
      fn into_value(self) -> Value;
      fn from_value(value: Value) -> Result<Self, Error> where Self: Sized;
  }
  ```
  `#[derive(SurrealValue)]` and `kind!` re-exported `types/src/lib.rs:24`.
- `kind!(…)` proc-macro — `types/derive/src/lib.rs:189`; DSL for `Kind` (`array<string>`, `record<user|post>`, `{a:int}`, escape hatch `(expr)`) doc `:152`. `Kind` enum `types/src/kind/mod.rs:17`; `Kind: ToSql` `:115` (e.g. `record<…>`, `array<…>`).
- Value helpers (stable): `object!`/`array!`/`set!`/`vars!` macros `types/src/lib.rs:46/136/91/115`; `RecordId{table,key}` + `RecordId::new` `types/src/value/record_id/mod.rs:22,31`; `ToSql for Value` `types/src/value/mod.rs:740`; `ToSql for RecordId` renders `table:key` `…/record_id/mod.rs:51`.

---

## 3. CRITICAL stability call — build on `ast`, on `core::sql::ToSql`, or on string templates?

| Option | Stable? | Renders SurrealQL? | Reachable? | Verdict |
|---|---|---|---|---|
| `surrealdb/ast` | **No** ("free to break between patch versions", `ast/src/lib.rs:6-12`) | **No** (debug only) | crate is internal | **Reject** |
| `core::sql::*Statement::ToSql` | No (core internal) | **Yes** | **No** — `pub(crate)`, not in SDK | **Reject** |
| **strings + `surrealdb-types` values/bind** | `ToSql` flagged experimental but `Value`/`Kind`/`SurrealValue`/SDK `query/bind` are the public SDK contract | n/a (you write the text) | **Yes** | **Adopt** |

**Recommendation:** the `nexgen` emitter renders SurrealQL **as templated strings** and runs them through the public SDK seam `Surreal::query(impl Into<Cow<str>>)` + `.bind(impl IntoVariables)` (`src/method/query.rs:112,253`). Data flows as **bound params** (`Value` via `SurrealValue`/`into_value`); only identifiers and DDL keywords/`Kind`s are interpolated, using `surrealdb-types::ToSql` (`Kind::to_sql`, `RecordId::to_sql`) + the escape utils (`EscapeSqonIdent`, `QuoteStr`) for the few literal fragments.

Why: (1) both ASTs are internal & version-unstable; the arena AST can't even print SurrealQL and the core printer is unreachable. (2) The string+bind path is exactly what the SDK exposes and is injection-safe for data. (3) `surrealdb-types` (`Value`,`Kind`,`SurrealValue`,`kind!`,`object!`) is the intended stable typing layer — lean on it for value marshalling, treat `ToSql` as a *convenience* for value/identifier rendering, not a load-bearing statement API. Pin the `surrealdb`/`surrealdb-types` versions and wrap `ToSql` use behind one adapter module so an experimental-API change is a one-file fix.

---

## 4. RELATE ≅ SPO-triple (`RELATE a -> pred -> b`)

Two representations:
- **Arena AST** `Relate` — `ast/src/lib.rs:498`:
  ```rust
  pub struct Relate{ pub only: bool,
      pub from: NodeId<Expr>,     // S (subject)
      pub through: NodeId<Expr>,  // P (predicate / edge table)
      pub to: NodeId<Expr>,       // O (object)
      pub data: Option<RecordData>, pub output: Option<NodeId<Output>>, pub timeout: Option<NodeId<Expr>> }
  ```
  Clean SPO triple: `from`=in, `through`=edge/predicate, `to`=out. Edge *properties* go in `data: RecordData` (`:406` — `Content/Set/Merge/…`). No text emission.
- **core owned AST** `RelateStatement` — `core/src/sql/statements/relate.rs:8` (`pub(crate)`): same `from`/`through`/`to: Expr` + `data: Option<Data>`. Its `ToSql` `:24` is the canonical printer:
  - `RELATE [ONLY] <from> -> <through> -> <to> [data] [output] [TIMEOUT t]`;
  - bare record-id/array/param subjects print unparenthesised, everything else is wrapped in `(...)` (`:34-89`); edge prints bare only for `Expr::Param|Expr::Table`.
- **Isomorphism for nexgen:** an SPO triple `(s,p,o)` maps to the string `RELATE $s -> p -> $o [CONTENT $props]` with `$s`,`$o`,`$props` **bound** as `RecordId`/`Object` values and `p` an interpolated (escaped) edge-table ident. `in`/`out` are the SurrealDB edge endpoints; `id` of the edge record is auto/declared. This is the holy-grail mapping and it falls out of plain templating — no AST dependency required.

---

## 5. Additive seam — what a `nexgen` emitter depends on
- **Depend on (public, stable):** `surrealdb-types` — `Value`, `Number`, `Object`, `Array`, `RecordId`, `Kind`, `SurrealValue` (+ derive), `kind!`, `object!`/`array!`; and the SDK `Surreal::query/bind`.
- **Use, but quarantine:** `surrealdb-types::ToSql`/`write_sql!`/`SqlFormat` + escape helpers — only for rendering identifiers / `Kind` / standalone values into DDL fragments. Keep behind a single `surql_fmt` adapter module.
- **Do NOT depend on:** anything in `surrealdb/ast` or `surrealdb/core` (`core::sql::*`, `core::expr::*`). Not public, not SemVer-stable.
- **Stays additive:** emitter = a new crate that *produces `String` + `Variables`*; it adds no trait impls to surreal types and patches nothing upstream. New statements/facets = new template functions. Mirrors the workspace "additive / no upstream edits" rule.

## Risks
1. **`ToSql` is `#[experimental]`** (`types/src/sql.rs:13`) — may change/vanish without a major bump. Mitigation: pin versions; isolate in one adapter; values flow via `bind`, so worst case is re-implementing identifier escaping.
2. **Two-AST confusion** — `surrealdb/ast` (named in brief) is *not* the renderer; `core::sql` is, but is `pub(crate)`/internal. Building on either couples nexgen to an explicitly unstable internal API.
3. **Manual SurrealQL templating** = own the grammar correctness (quoting/paren rules like RELATE's, reserved words). Mitigation: reuse `surrealdb-types` escape utils + a tight golden-string test suite; data is always bound, so injection risk is contained to identifiers.
4. **No parse-back / validation** in the chosen path — emitter can produce invalid SurrealQL that only fails at execution. Mitigation: integration tests that actually `query()` each emitted statement.
