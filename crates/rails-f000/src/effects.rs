// Copyright (C) 2012-2026 Zammad Foundation, https://zammad-foundation.org/

//! Callbacks are not population algebra.
//!
//! An F000 word is a fold over a population. It never authorizes a write, an
//! ordered lifecycle step, or a commit-stage effect. Every callback is
//! therefore classified into an [`EffectClass`] and answered with a refusal
//! or a deferral. There is deliberately no function here (or anywhere in this
//! crate) that turns a callback into a `MethodBinding`.
//!
//! The classes are OGAR's shared [`EffectClass`], so zammad-rs, odoo-rs, HIRO
//! and OGAR answer a behaviour with the same class. The classification below
//! stays zammad-rs's own: it reads the lowered body, which OGAR's
//! `classify(&ActionDef)` does not have.

use rails_compiler::{Binding, ChainEntry};

pub use ogar_vocab::effect_class::EffectClass;

use crate::transpose::{RefuseReason, Verdict};

/// What the lowered body of a callback is known to do.
#[derive(Debug, Clone, Default)]
pub struct LoweredFacts {
    /// The generated Rust text of the lowered fn, if available.
    pub fn_text: Option<String>,
    /// Fields ruff's `Function.writes` reports for the method.
    pub writes: Vec<String>,
}

impl LoweredFacts {
    #[must_use]
    pub fn writes_record(&self) -> bool {
        !self.writes.is_empty() || self.fn_text.as_deref().is_some_and(|t| t.contains(".set("))
    }

    /// The facts of the generated fn `fn_name` in the text of a generated
    /// model file.
    #[must_use]
    pub fn from_generated(source: &str, fn_name: &str) -> Self {
        let needle = format!("pub fn {fn_name}(");
        let fn_text = source.find(&needle).map(|at| {
            let rest = &source[at..];
            let end = rest.find("\n}\n").map_or(rest.len(), |e| e + 3);
            rest[..end].to_string()
        });
        Self { fn_text, writes: vec![] }
    }
}

fn boundary(phase: &str, why: &str) -> Verdict {
    Verdict::Refuse(RefuseReason::EffectBoundary { phase: phase.to_string(), why: why.to_string() })
}

/// Classify one entry of an effective callback chain.
#[must_use]
pub fn classify_callback(entry: &ChainEntry, facts: &LoweredFacts) -> (EffectClass, Verdict) {
    let phase = entry.phase.as_str();
    let class = classify_class(entry, facts);
    if cfg!(feature = "disable-effect-gate") {
        return (class, Verdict::Compose);
    }
    let verdict = match class {
        EffectClass::TransactionEffect => boundary(
            phase,
            "commit-stage effect: runs after the transaction, ordered by it; a population fold is neither",
        ),
        EffectClass::FrameworkGenerated => {
            boundary(phase, "ActiveRecord-generated callback: ordered by the save pipeline, per record")
        }
        EffectClass::Validation => boundary(phase, "validation adds errors to one record in chain order"),
        EffectClass::FieldTransform => boundary(phase, "writes a field of the record being saved; a fold never writes"),
        EffectClass::LifecycleWrite => boundary(phase, "writes during an ordered lifecycle step; a fold never writes"),
        EffectClass::ExternalHost => boundary(phase, "body is host-bound, outside what the lens can address"),
        EffectClass::PurePredicateCondition => Verdict::Defer("per-record scalar, not a population fold".to_string()),
        EffectClass::Unresolved => Verdict::Defer(match &entry.binding {
            Binding::Residual(why) => format!("body could not be lowered: {why}"),
            _ => "lowered body writes nothing the compiler can see, but it still runs per record in lifecycle order".to_string(),
        }),
        // The shared enum is non-exhaustive: a class added later is refused
        // until this table names it.
        other => boundary(phase, &format!("effect class {} has no answer here", other.name())),
    };
    (class, verdict)
}

fn classify_class(entry: &ChainEntry, facts: &LoweredFacts) -> EffectClass {
    let phase = entry.phase.as_str();
    if matches!(phase, "after_commit" | "after_rollback") {
        return EffectClass::TransactionEffect;
    }
    match &entry.binding {
        Binding::Framework => EffectClass::FrameworkGenerated,
        Binding::Validator(_) => EffectClass::Validation,
        Binding::Host => EffectClass::ExternalHost,
        Binding::Residual(_) => EffectClass::Unresolved,
        Binding::Lowered(_) => {
            if facts.writes_record() {
                if phase.starts_with("before_") || phase == "after_initialize" {
                    EffectClass::FieldTransform
                } else {
                    EffectClass::LifecycleWrite
                }
            } else if phase == "validate" || phase.ends_with("validation") {
                EffectClass::Validation
            } else {
                EffectClass::Unresolved
            }
        }
    }
}

/// Classify one `if:` / `unless:` condition of a callback, as the compiler
/// lowered it (`builtin:persisted?`, `fn:…`, `residual:…`).
#[must_use]
pub fn classify_condition(cond: &(String, String)) -> (EffectClass, Verdict) {
    let class = if cond.1.starts_with("builtin:") { EffectClass::PurePredicateCondition } else { EffectClass::Unresolved };
    if cfg!(feature = "disable-effect-gate") {
        return (class, Verdict::Compose);
    }
    (class, Verdict::Defer("per-record scalar, not a population fold".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(phase: &str, binding: Binding) -> ChainEntry {
        ChainEntry {
            phase: phase.into(),
            name: "x".into(),
            on: None,
            conds: vec![],
            binding,
            rule: "source".into(),
            provenance: "a.rb:1".into(),
            would_lower: None,
        }
    }

    fn writes() -> LoweredFacts {
        LoweredFacts { fn_text: Some("rec.set(col::A, v)".into()), writes: vec![] }
    }

    #[cfg(not(feature = "disable-effect-gate"))]
    #[test]
    fn every_class_is_a_refusal_or_a_deferral() {
        let cases = [
            (entry("after_commit", Binding::Host), LoweredFacts::default(), EffectClass::TransactionEffect),
            (entry("after_rollback", Binding::Lowered("m".into())), writes(), EffectClass::TransactionEffect),
            (entry("before_save", Binding::Framework), LoweredFacts::default(), EffectClass::FrameworkGenerated),
            (entry("validate", Binding::Validator("presence:a".into())), LoweredFacts::default(), EffectClass::Validation),
            (entry("before_create", Binding::Host), LoweredFacts::default(), EffectClass::ExternalHost),
            (entry("before_save", Binding::Lowered("m".into())), writes(), EffectClass::FieldTransform),
            (entry("after_create", Binding::Lowered("m".into())), writes(), EffectClass::LifecycleWrite),
            (entry("after_create", Binding::Lowered("m".into())), LoweredFacts::default(), EffectClass::Unresolved),
            (entry("after_create", Binding::Residual("ivar".into())), LoweredFacts::default(), EffectClass::Unresolved),
        ];
        for (e, f, want) in cases {
            let (class, verdict) = classify_callback(&e, &f);
            assert_eq!(class, want, "{} {:?}", e.phase, e.binding);
            assert!(matches!(verdict, Verdict::Refuse(RefuseReason::EffectBoundary { .. }) | Verdict::Defer(_)));
            assert!(!verdict.admits());
        }
    }

    #[cfg(not(feature = "disable-effect-gate"))]
    #[test]
    fn a_pure_condition_is_deferred_as_a_per_record_scalar() {
        let (class, verdict) = classify_condition(&("if".into(), "builtin:persisted?".into()));
        assert_eq!(class, EffectClass::PurePredicateCondition);
        assert_eq!(verdict, Verdict::Defer("per-record scalar, not a population fold".into()));
    }

    #[test]
    fn facts_are_read_from_generated_text() {
        let src = "/// a\npub fn m_a(x) {\n    rec.set(col::A, v)\n}\n\npub fn m_b(x) {\n    1\n}\n";
        assert!(LoweredFacts::from_generated(src, "m_a").writes_record());
        assert!(!LoweredFacts::from_generated(src, "m_b").writes_record());
        assert!(LoweredFacts::from_generated(src, "m_c").fn_text.is_none());
    }
}
