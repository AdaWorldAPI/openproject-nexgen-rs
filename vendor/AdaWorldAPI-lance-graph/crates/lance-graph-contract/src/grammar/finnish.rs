//! Finnish — 15 cases as direct slot lookups.
//!
//! Grammar-heavy languages are **easier**, not harder: case endings
//! directly encode slot semantics. A Finnish inessive `-ssa` means
//! "in X" (Lokal slot); an elative `-sta` means "from X" (Lokal source);
//! an accusative `-n` marks the object. No parser ambiguity: the
//! morphology tells you the role.

use super::{TekamoloSlot, WechselRole};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum FinnishCase {
    Nominative = 0, // subject
    Genitive = 1,   // -n   possessor / object (rules differ)
    Accusative = 2, // -n / -t  object
    Partitive = 3,  // -a / -ä / -ta partial / negated object
    // Interior locative
    Inessive = 4, // -ssa / -ssä  in
    Elative = 5,  // -sta / -stä  out of / from
    Illative = 6, // -Vn / -hVn / -seen  into
    // Exterior locative
    Adessive = 7, // -lla / -llä  at / on / by
    Ablative = 8, // -lta / -ltä  from
    Allative = 9, // -lle         to / onto
    // General
    Essive = 10,      // -na / -nä    as / in the state of
    Translative = 11, // -ksi         becoming
    Instructive = 12, // -in          by means of (rare)
    Abessive = 13,    // -tta / -ttä  without
    Comitative = 14,  // -ne          with (accompaniment)
}

impl FinnishCase {
    /// Which TEKAMOLO slot this case typically fills.
    pub fn tekamolo_hint(self) -> Option<TekamoloSlot> {
        match self {
            Self::Inessive
            | Self::Elative
            | Self::Illative
            | Self::Adessive
            | Self::Ablative
            | Self::Allative => Some(TekamoloSlot::Lokal),
            Self::Essive | Self::Translative => Some(TekamoloSlot::Modal),
            Self::Instructive | Self::Abessive | Self::Comitative => Some(TekamoloSlot::Modal),
            _ => None,
        }
    }

    /// Case's role when it disambiguates a Wechsel in surrounding English
    /// / German text. Used during cross-linguistic superposition.
    pub fn cross_lingual_role(self) -> Option<WechselRole> {
        match self {
            Self::Inessive | Self::Adessive => Some(WechselRole::PrepSpatial),
            Self::Elative | Self::Ablative => Some(WechselRole::PrepSpatial),
            Self::Illative | Self::Allative => Some(WechselRole::PrepSpatial),
            Self::Essive | Self::Translative => Some(WechselRole::PrepModal),
            _ => None,
        }
    }
}

/// Resolve a Finnish case from a token suffix (last 1–4 chars, lowercased).
///
/// Returns `None` for unknown / nominative-default. Order of checks matters:
/// longer suffixes first so `-ssa` doesn't get picked up as `-a`.
pub fn finnish_case_for_suffix(suffix: &str) -> Option<FinnishCase> {
    let s = suffix;
    // 4+ char suffixes first
    if s.ends_with("seen") {
        return Some(FinnishCase::Illative);
    }
    // 3-char suffixes
    if s.ends_with("sta") || s.ends_with("stä") {
        return Some(FinnishCase::Elative);
    }
    if s.ends_with("ssa") || s.ends_with("ssä") {
        return Some(FinnishCase::Inessive);
    }
    if s.ends_with("lla") || s.ends_with("llä") {
        return Some(FinnishCase::Adessive);
    }
    if s.ends_with("lta") || s.ends_with("ltä") {
        return Some(FinnishCase::Ablative);
    }
    if s.ends_with("lle") {
        return Some(FinnishCase::Allative);
    }
    if s.ends_with("tta") || s.ends_with("ttä") {
        return Some(FinnishCase::Abessive);
    }
    // 2-char suffixes
    if s.ends_with("na") || s.ends_with("nä") {
        return Some(FinnishCase::Essive);
    }
    if s.ends_with("ne") {
        return Some(FinnishCase::Comitative);
    }
    if s.ends_with("ta") || s.ends_with("tä") {
        return Some(FinnishCase::Partitive);
    }
    if s.ends_with("in") {
        return Some(FinnishCase::Instructive);
    }
    // 3-char 'ksi' before 2-char 'si' heuristic
    if s.ends_with("ksi") {
        return Some(FinnishCase::Translative);
    }
    // 1-char suffixes (ambiguous — return conservative)
    if s.ends_with('n') {
        return Some(FinnishCase::Genitive);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inessive_suffix() {
        assert_eq!(
            finnish_case_for_suffix("talossa"),
            Some(FinnishCase::Inessive)
        );
        assert_eq!(
            finnish_case_for_suffix("metsässä"),
            Some(FinnishCase::Inessive)
        );
    }

    #[test]
    fn elative_suffix() {
        assert_eq!(
            finnish_case_for_suffix("talosta"),
            Some(FinnishCase::Elative)
        );
    }

    #[test]
    fn illative_seen_matches_before_n() {
        assert_eq!(
            finnish_case_for_suffix("huoneeseen"),
            Some(FinnishCase::Illative)
        );
    }

    #[test]
    fn lokal_hint_for_interior_cases() {
        use super::TekamoloSlot;
        assert_eq!(
            FinnishCase::Inessive.tekamolo_hint(),
            Some(TekamoloSlot::Lokal)
        );
        assert_eq!(
            FinnishCase::Elative.tekamolo_hint(),
            Some(TekamoloSlot::Lokal)
        );
        assert_eq!(
            FinnishCase::Illative.tekamolo_hint(),
            Some(TekamoloSlot::Lokal)
        );
    }
}
