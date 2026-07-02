//! `emission_scan` — DDL-type-expression counting logic (zero-dep).
//!
//! Requested by the op-nexgen consumer session (post-#630 wishlist L2) so
//! every consumer measures typed-DDL schema coverage identically instead of
//! grepping its own emitted DDL; sibling of [`classid_scan`](crate::classid_scan)
//! (D-V3-W6a). Reference figure at request time: nexgen measured 89.5% typed
//! fields on the OpenProject corpus by hand-grep.
//!
//! This module mirrors `classid_scan`'s two-piece shape: [`classify_ddl_type`]
//! buckets one DDL type expression (a `DEFINE FIELD ... TYPE <expr>` right-hand
//! side) into a [`TypedForm`], [`count_emission`] folds an iterator of such
//! expressions into [`EmissionCounts`]. No classid handling of any kind lives
//! here — this module counts *type expressions*, not classids, and performs no
//! bit math on any composed `u32` (`/v3-audit` check 1 does not apply — there is
//! nothing here for it to flag).
//!
//! Classification is a deterministic tokenizer walk over the expression's
//! alphanumeric runs, in fixed PRECEDENCE order: [`TypedForm::Stub`] >
//! [`TypedForm::RecordLink`] > [`TypedForm::AnyTyped`] > [`TypedForm::Typed`].
//! Tokenizing on non-alphanumeric boundaries means a token equal to `any` only
//! matches the bare word `any` (not a substring of `many`), and a token equal
//! to `record` only matches the bare word `record` (not a substring of
//! `recording`) — see [`classify_ddl_type`]'s doc comment for the full worked
//! example table.
//!
//! # The contract scan-family pattern (named 2026-07-02)
//!
//! This is the SECOND instance of a named design language: a governance
//! metric implemented as a zero-dep contract fold — a `Form` enum +
//! `classify_*` per item + `count_*` fold to a counts struct. Instances:
//! [`classid_scan`](crate::classid_scan) (V3 classid adoption) and this
//! module (typed-DDL adoption). Ratified by three-session convergence
//! (board: E-V3-XSESSION-INTAKE-1): the NEXT governance counter
//! (soc-verdict counts, predicate coverage, parity-fixture coverage, ...)
//! MIRRORS this shape in the contract instead of living as a consumer-side
//! grep — a bespoke grep where a scan module belongs is the drift signal.

/// The decoded shape of one SurrealQL DDL `TYPE` expression, per the
/// precedence procedure in [`classify_ddl_type`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum TypedForm {
    /// A concrete SurrealQL type expression: `int`, `float`, `bool`,
    /// `string`, `datetime`, `duration`, `bytes`, `number`, `decimal`,
    /// `object`, `geometry`, `uuid`, or an `array<T>` / `set<T>` /
    /// `option<T>` wrapping a concrete `T`. Worked examples: `"int"`,
    /// `"array<float>"`.
    Typed,
    /// The expression's effective type is `any` — bare `any`, or
    /// `array<any>` / `set<any>` / `option<any>` (schema present but
    /// typeless). Worked examples: `"any"`, `"array<any>"`.
    AnyTyped,
    /// The expression contains a `record` link type (`record<...>`, or
    /// nested inside another wrapper, e.g. `array<record<user>>`). Worked
    /// examples: `"record<work_package>"`, `"array<record<user>>"`.
    RecordLink,
    /// No usable type: an empty or whitespace-only expression, or a
    /// placeholder marker (`todo` / `stub` / `fixme`, case-insensitive, as a
    /// standalone token). Worked examples: `""`, `"TODO"`.
    Stub,
}

/// Classify one DDL type expression into its [`TypedForm`], by precedence
/// **Stub > RecordLink > AnyTyped > Typed**.
///
/// Tokenizes the expression on non-alphanumeric boundaries (`<`, `>`, `|`,
/// whitespace, etc.), so a token is only ever a maximal run of ASCII
/// alphanumerics — `many` never matches the `any` token test, and `recording`
/// never matches the `record` token test.
///
/// - Empty/whitespace-only expression, or any token case-insensitively equal
///   to `todo` / `stub` / `fixme` → [`TypedForm::Stub`].
/// - Otherwise, any token equal to `record` (any case run — SurrealQL type
///   keywords are lowercase, matched case-sensitively like the rest of this
///   classifier) → [`TypedForm::RecordLink`].
/// - Otherwise, any token equal to `any` → [`TypedForm::AnyTyped`].
/// - Otherwise → [`TypedForm::Typed`].
///
/// Worked examples (doc-pinned, mirrored in `#[test]`s below):
///
/// | Expression              | Result               |
/// |--------------------------|----------------------|
/// | `"int"`                  | `Typed`               |
/// | `"array<float>"`         | `Typed`               |
/// | `"any"`                  | `AnyTyped`             |
/// | `"array<any>"`           | `AnyTyped`             |
/// | `"record<work_package>"` | `RecordLink`           |
/// | `"array<record<user>>"`  | `RecordLink`           |
/// | `""`                     | `Stub`                 |
/// | `"TODO"`                 | `Stub`                 |
#[inline]
#[must_use]
pub fn classify_ddl_type(ty: &str) -> TypedForm {
    let mut saw_record = false;
    let mut saw_any = false;
    let mut token_count = 0usize;

    for token in ty.split(|c: char| !c.is_ascii_alphanumeric() && c != '_') {
        if token.is_empty() {
            continue;
        }
        token_count += 1;

        // Stub is the ONLY early return: it is top precedence, so nothing a
        // later token could contain outranks it. `record`/`any` must NOT
        // early-return — a stub marker may still follow (e.g.
        // `record<user> TODO`, `record<fixme>`), and the documented
        // precedence says Stub wins globally, not first-token-wins
        // (codex P2, PR #632).
        if token.eq_ignore_ascii_case("todo")
            || token.eq_ignore_ascii_case("stub")
            || token.eq_ignore_ascii_case("fixme")
        {
            return TypedForm::Stub;
        }
        if token == "record" {
            saw_record = true;
        }
        if token == "any" {
            saw_any = true;
        }
    }

    if token_count == 0 {
        // Empty or whitespace-only expression.
        return TypedForm::Stub;
    }
    if saw_record {
        return TypedForm::RecordLink;
    }
    if saw_any {
        return TypedForm::AnyTyped;
    }
    TypedForm::Typed
}

/// Range-count tallies over a scanned set of DDL type expressions, mirroring
/// `classid_scan::AdoptionCounts`'s field shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EmissionCounts {
    /// Rows classified as [`TypedForm::Typed`].
    pub typed: u64,
    /// Rows classified as [`TypedForm::AnyTyped`].
    pub any_typed: u64,
    /// Rows classified as [`TypedForm::RecordLink`].
    pub record_link: u64,
    /// Rows classified as [`TypedForm::Stub`].
    pub stub: u64,
}

impl EmissionCounts {
    /// Total rows observed (`typed + any_typed + record_link + stub`).
    #[inline]
    #[must_use]
    pub fn total(&self) -> u64 {
        self.typed + self.any_typed + self.record_link + self.stub
    }

    /// Fold one classified [`TypedForm`] into the running tallies.
    #[inline]
    pub fn observe(&mut self, form: TypedForm) {
        match form {
            TypedForm::Typed => self.typed += 1,
            TypedForm::AnyTyped => self.any_typed += 1,
            TypedForm::RecordLink => self.record_link += 1,
            TypedForm::Stub => self.stub += 1,
        }
    }

    /// Typed-coverage ratio: `typed / total`, in `[0.0, 1.0]`. `0.0` for an
    /// empty scan (`total() == 0`) rather than `NaN` — mirrors
    /// `classid_scan::AdoptionCounts::adoption_pct`'s "empty corpus is
    /// vacuously not-yet-typed, not undefined" convention exactly (that
    /// method also returns `f64`, so this crate's zero-dep constraint does
    /// not forbid floating point — see the module-level report note on why
    /// this deviates from an integer-permille shape).
    #[inline]
    #[must_use]
    pub fn typed_ratio(&self) -> f64 {
        let total = self.total();
        if total == 0 {
            0.0
        } else {
            self.typed as f64 / total as f64
        }
    }
}

/// Fold an iterator of DDL type expressions into [`EmissionCounts`] by
/// [`classify_ddl_type`]. Mirrors `classid_scan::count_adoption`'s signature
/// shape (`impl Iterator<Item = T>`, not `IntoIterator`) over `&str` items.
#[must_use]
pub fn count_emission<'a>(types: impl Iterator<Item = &'a str>) -> EmissionCounts {
    let mut counts = EmissionCounts::default();
    for ty in types {
        counts.observe(classify_ddl_type(ty));
    }
    counts
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── classify_ddl_type: doc-pinned worked examples ──

    #[test]
    fn classify_ddl_type_int_is_typed() {
        assert_eq!(classify_ddl_type("int"), TypedForm::Typed);
    }

    #[test]
    fn classify_ddl_type_array_float_is_typed() {
        assert_eq!(classify_ddl_type("array<float>"), TypedForm::Typed);
    }

    #[test]
    fn classify_ddl_type_bare_any_is_any_typed() {
        assert_eq!(classify_ddl_type("any"), TypedForm::AnyTyped);
    }

    #[test]
    fn classify_ddl_type_array_any_is_any_typed() {
        assert_eq!(classify_ddl_type("array<any>"), TypedForm::AnyTyped);
    }

    #[test]
    fn classify_ddl_type_record_link_is_record_link() {
        assert_eq!(
            classify_ddl_type("record<work_package>"),
            TypedForm::RecordLink
        );
    }

    #[test]
    fn classify_ddl_type_nested_array_record_is_record_link() {
        assert_eq!(
            classify_ddl_type("array<record<user>>"),
            TypedForm::RecordLink
        );
    }

    #[test]
    fn classify_ddl_type_empty_is_stub() {
        assert_eq!(classify_ddl_type(""), TypedForm::Stub);
    }

    #[test]
    fn classify_ddl_type_whitespace_only_is_stub() {
        assert_eq!(classify_ddl_type("   "), TypedForm::Stub);
    }

    #[test]
    fn classify_ddl_type_todo_marker_is_stub() {
        assert_eq!(classify_ddl_type("TODO"), TypedForm::Stub);
        assert_eq!(classify_ddl_type("todo"), TypedForm::Stub);
        assert_eq!(classify_ddl_type("stub"), TypedForm::Stub);
        assert_eq!(classify_ddl_type("fixme"), TypedForm::Stub);
    }

    // ── precedence tests ──

    #[test]
    fn classify_ddl_type_record_any_is_record_link_not_any_typed() {
        // record<any> — precedence: RecordLink > AnyTyped, both tokens present.
        assert_eq!(classify_ddl_type("record<any>"), TypedForm::RecordLink);
    }

    #[test]
    fn classify_ddl_type_stub_marker_beats_record_and_any() {
        // A stub marker anywhere in the expression wins, even alongside
        // record/any tokens.
        assert_eq!(classify_ddl_type("TODO record<any>"), TypedForm::Stub);
    }

    #[test]
    fn classify_ddl_type_stub_marker_after_record_still_wins() {
        // Regression (codex P2, PR #632): a stub marker AFTER the `record`
        // token must still win — precedence is global over the whole
        // expression, never first-token-wins. Before the fix, the early
        // return on `record` miscounted partially-stubbed record-link DDL
        // as real links.
        assert_eq!(classify_ddl_type("record<user> TODO"), TypedForm::Stub);
        assert_eq!(classify_ddl_type("record<fixme>"), TypedForm::Stub);
        assert_eq!(
            classify_ddl_type("array<record<user>> stub"),
            TypedForm::Stub
        );
    }

    #[test]
    fn classify_ddl_type_false_positive_guard_many_and_recording() {
        // Substring "any" inside "many" and substring "record" inside
        // "recording" must NOT trigger the corresponding classification —
        // tokenization is on non-alphanumeric boundaries only.
        assert_eq!(classify_ddl_type("many"), TypedForm::Typed);
        assert_eq!(classify_ddl_type("recording"), TypedForm::Typed);
        assert_eq!(classify_ddl_type("array<recording>"), TypedForm::Typed);
    }

    // ── EmissionCounts / count_emission ──

    #[test]
    fn count_emission_mixed_produces_correct_tallies_and_ratio() {
        let types = [
            "int",                  // Typed
            "array<float>",         // Typed
            "any",                  // AnyTyped
            "array<any>",           // AnyTyped
            "record<work_package>", // RecordLink
            "array<record<user>>",  // RecordLink
            "",                     // Stub
            "TODO",                 // Stub
        ];
        let counts = count_emission(types.into_iter());
        assert_eq!(
            counts,
            EmissionCounts {
                typed: 2,
                any_typed: 2,
                record_link: 2,
                stub: 2,
            }
        );
        assert_eq!(counts.total(), 8);
        assert!((counts.typed_ratio() - 0.25).abs() < f64::EPSILON);
    }

    #[test]
    fn count_emission_all_typed_is_full_ratio() {
        let types = ["int", "bool", "string"];
        let counts = count_emission(types.into_iter());
        assert_eq!(counts.total(), 3);
        assert!((counts.typed_ratio() - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn count_emission_empty_iterator_is_zero_not_nan() {
        let counts = count_emission(std::iter::empty());
        assert_eq!(counts, EmissionCounts::default());
        assert_eq!(counts.total(), 0);
        assert_eq!(counts.typed_ratio(), 0.0);
        assert!(!counts.typed_ratio().is_nan());
    }
}
