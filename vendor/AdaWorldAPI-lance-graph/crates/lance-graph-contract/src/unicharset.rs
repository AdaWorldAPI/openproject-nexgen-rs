//! `UNICHARSET` content store — the Rust side of the byte-parity probe
//! (`PROBE-OGAR-ADAPTER-UNICHARSET`).
//!
//! Tesseract's `UNICHARSET` is a variable-length id↔unichar bijection loaded
//! from a `.unicharset` text file. Per the Core-First doctrine it is NOT
//! fixed-width per-node state — it rides a **classid-keyed content-store tier**
//! shaped exactly like `deepnsm::Vocabulary`: a `reverse: Vec<String>`
//! (id → unichar) plus a `lookup: HashMap<String, u32>` (unichar → id). This
//! module is that tier plus the two adapter leaves (`id_to_unichar` /
//! `unichar_to_id`).
//!
//! # Why this is the byte-parity surface
//!
//! The unicharset path is pure text parsing — it never touches leptonica or
//! `Pix`. So the Rust side can be built and tested with **zero C dependencies**.
//! The probe compares this implementation's [`UniCharSet::dump`] of a real
//! `eng.unicharset` against the C++ `UNICHARSET::id_to_unichar` oracle (a small
//! libtesseract harness, which only *links* leptonica, never calls it). Byte-
//! identical dumps promote the doctrine CONJECTURE → FINDING.
//!
//! # Format scope
//!
//! The `.unicharset` format is: line 1 = entry count `N`; then `N` lines, each
//! beginning with the unichar as its first whitespace-delimited token, followed
//! by the **properties** as the second token (a hex bitmask), then script /
//! bounding boxes / case / direction columns. The line position (0-based, after
//! the count line) IS the unichar id. This is the `old_style_included_ == true`
//! plain-table scope the adapter-shaper bounded; fragment/`CleanupString`
//! normalization is a separate, later leaf. Any special-token edge case a real
//! `eng.unicharset` reveals on first diff is refined then — this is built to the
//! documented format, diff-pending.
//!
//! # Properties leaf
//!
//! The second token is a hex bitmask (tesseract `unicharset.cpp:824`,
//! `stream >> std::hex >> properties`) decoded by `set_is*(id, properties & MASK)`
//! at `unicharset.cpp:888-892`. The masks (`unicharset.cpp:41-45`) are
//! `ISALPHA=0x1 ISLOWER=0x2 ISUPPER=0x4 ISDIGIT=0x8 ISPUNCTUATION=0x10`. The
//! [`UniCharSet::get_isalpha`] family mirrors the C++ accessors
//! (`unicharset.h:497+`): an out-of-range id (the C++ `INVALID_UNICHAR_ID`
//! sentinel) returns `false`, else the stored bit. `is_ngram` is never set by
//! the plain-table loader (`unicharset.cpp:893` always `set_isngram(id, false)`)
//! so [`UniCharSet::get_isngram`] is always `false` for a file-loaded set.
//! [`UniCharSet::dump_properties`] is the byte-parity surface for these bits.
//!
//! # Script leaf
//!
//! Each entry carries a script id — an index into an interned table built by
//! `add_script` (tesseract `unicharset.cpp:1063`, insertion-order dedup).
//! `null_script` ("NULL", `unicharset.cpp:82`) is seeded at id 0 by the first
//! `unichar_insert` (`unicharset.cpp:680`, before the real script is set), so
//! `null_sid_ == 0` always and real scripts follow in first-seen id order.
//! [`UniCharSet::get_script`] mirrors the C++ accessor (`unicharset.h:681`):
//! out-of-range id → `null_sid_` (0). The script name parsed per line is the
//! token after the optional bbox/stats CSV; [`UniCharSet::dump_script`] is the
//! byte-parity surface for the script ids.
//!
//! # Other-case leaf
//!
//! Each entry carries `other_case` — the id of its case-paired unichar (`'C'` →
//! `'c'`), or itself when there is no pair. It is the token immediately after
//! the script, clamped at load exactly as `unicharset.cpp:901`: a parsed value
//! not less than `size` (and the absent default, `unicharset.cpp:813`, = `size`)
//! folds to the id itself. [`UniCharSet::get_other_case`] mirrors the C++
//! accessor (`unicharset.h:703`): out-of-range id → `INVALID_UNICHAR_ID` (-1).
//! [`UniCharSet::dump_other_case`] is the byte-parity surface.

use std::collections::HashMap;
use std::path::Path;

/// A loaded `UNICHARSET`: the id↔unichar bijection, `deepnsm::Vocabulary`-shaped.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UniCharSet {
    /// id → unichar (index IS the id).
    reverse: Vec<String>,
    /// unichar → id (the inverse of `reverse`).
    lookup: HashMap<String, u32>,
    /// id → property bitmask (`ISALPHA|ISLOWER|ISUPPER|ISDIGIT|ISPUNCTUATION`),
    /// parallel to `reverse`. Only the low 5 bits are meaningful; see the
    /// `*_MASK` consts.
    props: Vec<u8>,
    /// id → script id (index into `scripts`), parallel to `reverse`. The C++
    /// `unichars[id].properties.script_id` (tesseract `unicharset.cpp:894`).
    script_ids: Vec<i32>,
    /// The interned script-name table (`add_script`, tesseract
    /// `unicharset.cpp:1063`). Index 0 is always `null_script` ("NULL"), seeded
    /// by the first `unichar_insert` (`unicharset.cpp:680`); real scripts follow
    /// in first-seen id order. `script_ids` indexes into this.
    scripts: Vec<String>,
    /// id → the case-paired unichar id (`other_case`), parallel to `reverse`.
    /// Clamped at load: a parsed value `>= size` (incl. the default) becomes the
    /// id itself (tesseract `unicharset.cpp:901`).
    other_cases: Vec<i32>,
}

/// `isalpha` property bit (tesseract `unicharset.cpp:41`).
const ISALPHA_MASK: u8 = 0x1;
/// `islower` property bit (tesseract `unicharset.cpp:42`).
const ISLOWER_MASK: u8 = 0x2;
/// `isupper` property bit (tesseract `unicharset.cpp:43`).
const ISUPPER_MASK: u8 = 0x4;
/// `isdigit` property bit (tesseract `unicharset.cpp:44`).
const ISDIGIT_MASK: u8 = 0x8;
/// `ispunctuation` property bit (tesseract `unicharset.cpp:45`).
const ISPUNCTUATION_MASK: u8 = 0x10;
/// All meaningful property bits — the loader masks the parsed hex to these.
const PROPERTY_BITS: u8 =
    ISALPHA_MASK | ISLOWER_MASK | ISUPPER_MASK | ISDIGIT_MASK | ISPUNCTUATION_MASK;

/// The null script name (tesseract `unicharset.cpp:82`), always interned at
/// script id 0 (`null_sid_ == 0`, `unicharset.cpp:949-950`).
const NULL_SCRIPT: &str = "NULL";
/// The null script id — what `get_script` returns for an out-of-range id
/// (the C++ `null_sid_`, `unicharset.h:683`).
const NULL_SID: i32 = 0;
/// The C++ `INVALID_UNICHAR_ID` sentinel — what id-returning accessors yield for
/// an out-of-range id (tesseract `unichar.h`; e.g. `get_other_case`).
const INVALID_UNICHAR_ID: i32 = -1;

/// Intern `name` into `scripts` (insertion-order dedup), returning its index —
/// the transcription of `UNICHARSET::add_script` (tesseract `unicharset.cpp:1063`).
fn intern_script(scripts: &mut Vec<String>, name: &str) -> i32 {
    let idx = scripts.iter().position(|s| s == name).unwrap_or_else(|| {
        scripts.push(name.to_string());
        scripts.len() - 1
    });
    i32::try_from(idx).unwrap_or(NULL_SID)
}

impl UniCharSet {
    /// Parse a `.unicharset` from its text contents. See the module docs for the
    /// format. Properties columns after the leading unichar token are ignored.
    ///
    /// # Errors
    ///
    /// [`UniCharSetError::Empty`] if there is no count line,
    /// [`UniCharSetError::BadCount`] if it is not a non-negative integer, and
    /// [`UniCharSetError::CountMismatch`] if fewer than `count` entry lines
    /// follow.
    pub fn load_from_str(text: &str) -> Result<Self, UniCharSetError> {
        let mut lines = text.lines();
        let count: usize = lines
            .next()
            .ok_or(UniCharSetError::Empty)?
            .trim()
            .parse()
            .map_err(|_| UniCharSetError::BadCount)?;

        let mut reverse = Vec::with_capacity(count);
        let mut lookup = HashMap::with_capacity(count);
        let mut props = Vec::with_capacity(count);
        let mut script_ids = Vec::with_capacity(count);
        let mut scripts: Vec<String> = Vec::new();
        let mut other_cases = Vec::with_capacity(count);
        let count_i32 = i32::try_from(count).unwrap_or(i32::MAX);
        for line in lines.take(count) {
            // The unichar is the first whitespace-delimited token; the id is the
            // entry's position. A unichar repeated in the file keeps its FIRST
            // id in `lookup` (matches a forward-scan loader), but `reverse` keeps
            // every entry so `id_to_unichar` is exact per position.
            //
            // The one special token: tesseract stores the space unichar as the
            // literal `"NULL"` (a real space can't be a whitespace-delimited
            // token), and load remaps `"NULL"` -> `" "` (tesseract
            // `unicharset.cpp:882`). The byte-parity probe surfaced this as the
            // sole id-0 diff against the C++ oracle.
            let mut tokens = line.split_whitespace();
            let token = tokens.next().unwrap_or("");
            let unichar = if token == "NULL" { " " } else { token }.to_string();
            // The second token is the property bitmask in hex (tesseract
            // `unicharset.cpp:824`). Parse leniently — a missing/!hex token means
            // "no properties" (0), matching this loader's documented tolerance for
            // partial lines; a well-formed `eng.unicharset` always supplies it, so
            // the byte-parity diff is unaffected. Mask to the 5 meaningful bits
            // exactly as `set_is*(id, properties & MASK)` does downstream.
            let properties = tokens
                .next()
                .and_then(|t| u32::from_str_radix(t, 16).ok())
                .unwrap_or(0);
            let id = u32::try_from(reverse.len()).map_err(|_| UniCharSetError::BadCount)?;
            lookup.entry(unichar.clone()).or_insert(id);
            reverse.push(unichar);
            // `try_from` always succeeds here (the mask bounds the value to
            // <= 0x1F); the fallback keeps the path total without `unwrap`.
            let prop_byte = u8::try_from(properties & u32::from(PROPERTY_BITS)).unwrap_or(0);
            props.push(prop_byte);
            // Script (tesseract `unicharset.cpp:894`). `unichar_insert` seeds
            // `null_script` at sid 0 (`unicharset.cpp:680`) BEFORE the real
            // script is set, so intern "NULL" first every iteration (idempotent
            // after the first) then the parsed script. The script token is the
            // one right after the optional bbox/stats CSV: token[2] when it has
            // no comma, else token[3] — covering every column tier present on
            // real `eng` data; an absent token defaults to `null_script`.
            intern_script(&mut scripts, NULL_SCRIPT);
            let script_name = match tokens.next() {
                Some(t) if t.contains(',') => tokens.next().unwrap_or(NULL_SCRIPT),
                Some(t) => t,
                None => NULL_SCRIPT,
            };
            script_ids.push(intern_script(&mut scripts, script_name));
            // other_case: the token right after the script (present in every
            // column tier that carries it). Clamped exactly as
            // `unicharset.cpp:901`: a parsed value not less than `size` — and the
            // absent default (`unicharset.cpp:813`, = size) — folds to the id.
            let id_i32 = i32::try_from(id).unwrap_or(INVALID_UNICHAR_ID);
            let oc = tokens
                .next()
                .and_then(|t| t.parse::<i32>().ok())
                .unwrap_or(count_i32);
            other_cases.push(if oc < count_i32 { oc } else { id_i32 });
        }

        if reverse.len() != count {
            return Err(UniCharSetError::CountMismatch {
                declared: count,
                found: reverse.len(),
            });
        }
        Ok(Self {
            reverse,
            lookup,
            props,
            script_ids,
            scripts,
            other_cases,
        })
    }

    /// Parse a `.unicharset` file from disk (a thin wrapper over
    /// [`Self::load_from_str`]).
    ///
    /// # Errors
    ///
    /// [`UniCharSetError::Io`] if the file cannot be read, else the parse errors
    /// of [`Self::load_from_str`].
    pub fn load_from_file(path: &Path) -> Result<Self, UniCharSetError> {
        let text = std::fs::read_to_string(path).map_err(|e| UniCharSetError::Io(e.to_string()))?;
        Self::load_from_str(&text)
    }

    /// Number of entries (the declared count).
    #[must_use]
    pub fn size(&self) -> usize {
        self.reverse.len()
    }

    /// The unichar string at `id`, or `None` if out of range. The C++ oracle
    /// for the byte-parity diff.
    #[must_use]
    pub fn id_to_unichar(&self, id: u32) -> Option<&str> {
        self.reverse.get(id as usize).map(String::as_str)
    }

    /// The id of `unichar`, or `None` if absent (the C++ `INVALID_UNICHAR_ID`
    /// sentinel maps to `None`; the OGAR adapter boundary re-applies the
    /// sentinel).
    #[must_use]
    pub fn unichar_to_id(&self, unichar: &str) -> Option<u32> {
        self.lookup.get(unichar).copied()
    }

    /// Test property bit `mask` for `id`. An out-of-range `id` (the C++
    /// `INVALID_UNICHAR_ID` sentinel) returns `false`, mirroring the C++
    /// accessor guard at `unicharset.h:497+`.
    fn has_property(&self, id: u32, mask: u8) -> bool {
        self.props.get(id as usize).is_some_and(|&p| p & mask != 0)
    }

    /// Whether `id` is alphabetic (`ISALPHA`). Out-of-range → `false`.
    /// Mirrors `UNICHARSET::get_isalpha` (tesseract `unicharset.h`).
    #[must_use]
    pub fn get_isalpha(&self, id: u32) -> bool {
        self.has_property(id, ISALPHA_MASK)
    }

    /// Whether `id` is lower-case (`ISLOWER`). Out-of-range → `false`.
    /// Mirrors `UNICHARSET::get_islower`.
    #[must_use]
    pub fn get_islower(&self, id: u32) -> bool {
        self.has_property(id, ISLOWER_MASK)
    }

    /// Whether `id` is upper-case (`ISUPPER`). Out-of-range → `false`.
    /// Mirrors `UNICHARSET::get_isupper`.
    #[must_use]
    pub fn get_isupper(&self, id: u32) -> bool {
        self.has_property(id, ISUPPER_MASK)
    }

    /// Whether `id` is a digit (`ISDIGIT`). Out-of-range → `false`.
    /// Mirrors `UNICHARSET::get_isdigit`.
    #[must_use]
    pub fn get_isdigit(&self, id: u32) -> bool {
        self.has_property(id, ISDIGIT_MASK)
    }

    /// Whether `id` is punctuation (`ISPUNCTUATION`). Out-of-range → `false`.
    /// Mirrors `UNICHARSET::get_ispunctuation`.
    #[must_use]
    pub fn get_ispunctuation(&self, id: u32) -> bool {
        self.has_property(id, ISPUNCTUATION_MASK)
    }

    /// Whether `id` is an n-gram. The plain-table loader always clears this
    /// (`unicharset.cpp:893`), so a file-loaded set returns `false` for every
    /// id; this mirrors `UNICHARSET::get_isngram` for that load path.
    #[must_use]
    pub fn get_isngram(&self, _id: u32) -> bool {
        false
    }

    /// The script id of `id` — an index into the interned script table. Mirrors
    /// `UNICHARSET::get_script` (tesseract `unicharset.h:681`): an out-of-range
    /// id (the `INVALID_UNICHAR_ID` sentinel) returns the null script id
    /// (`null_sid_ == 0`), else the stored `script_id`.
    #[must_use]
    pub fn get_script(&self, id: u32) -> i32 {
        self.script_ids
            .get(id as usize)
            .copied()
            .unwrap_or(NULL_SID)
    }

    /// The number of interned scripts (always ≥ 1: index 0 is `null_script`).
    /// Mirrors `UNICHARSET::get_script_table_size`.
    #[must_use]
    pub fn get_script_table_size(&self) -> usize {
        self.scripts.len()
    }

    /// The script name for a script id, or `None` if out of range. Mirrors
    /// `UNICHARSET::get_script_from_script_id` (which returns `null_script` for
    /// an out-of-range id; here the caller chooses the fallback via `Option`).
    #[must_use]
    pub fn script_from_script_id(&self, script_id: i32) -> Option<&str> {
        usize::try_from(script_id)
            .ok()
            .and_then(|i| self.scripts.get(i))
            .map(String::as_str)
    }

    /// The script name of unichar `id` (the composition of [`Self::get_script`]
    /// and [`Self::script_from_script_id`]) — the OCR-facing "what script is this
    /// character" query. Out-of-range → `Some("NULL")` (the null script).
    #[must_use]
    pub fn script_of(&self, id: u32) -> Option<&str> {
        self.script_from_script_id(self.get_script(id))
    }

    /// The case-paired unichar id of `id` (`other_case`) — e.g. `'C'` → the id of
    /// `'c'`, or the id itself when there is no pair. Mirrors
    /// `UNICHARSET::get_other_case` (tesseract `unicharset.h:703`): an
    /// out-of-range id (the `INVALID_UNICHAR_ID` sentinel) returns
    /// `INVALID_UNICHAR_ID` (-1).
    #[must_use]
    pub fn get_other_case(&self, id: u32) -> i32 {
        self.other_cases
            .get(id as usize)
            .copied()
            .unwrap_or(INVALID_UNICHAR_ID)
    }

    /// Render the id→properties table as
    /// `"<id>\t<isalpha> <islower> <isupper> <isdigit> <ispunctuation>\n"` lines
    /// (each flag `0`/`1`) — the exact shape the C++ property oracle prints, so
    /// the byte-parity diff is `diff oracle_props.tsv rust_props.tsv`.
    #[must_use]
    pub fn dump_properties(&self) -> String {
        let mut out = String::new();
        for id in 0..self.reverse.len() as u32 {
            out.push_str(&id.to_string());
            out.push('\t');
            out.push(if self.get_isalpha(id) { '1' } else { '0' });
            out.push(' ');
            out.push(if self.get_islower(id) { '1' } else { '0' });
            out.push(' ');
            out.push(if self.get_isupper(id) { '1' } else { '0' });
            out.push(' ');
            out.push(if self.get_isdigit(id) { '1' } else { '0' });
            out.push(' ');
            out.push(if self.get_ispunctuation(id) { '1' } else { '0' });
            out.push('\n');
        }
        out
    }

    /// Render the id→script-id table as `"<id>\t<script_id>\n"` lines — the exact
    /// shape the C++ `get_script` oracle prints, so the byte-parity diff is
    /// `diff oracle_script.tsv rust_script.tsv`.
    #[must_use]
    pub fn dump_script(&self) -> String {
        let mut out = String::new();
        for id in 0..self.reverse.len() as u32 {
            out.push_str(&id.to_string());
            out.push('\t');
            out.push_str(&self.get_script(id).to_string());
            out.push('\n');
        }
        out
    }

    /// Render the id→other-case table as `"<id>\t<other_case>\n"` lines — the
    /// exact shape the C++ `get_other_case` oracle prints, so the byte-parity
    /// diff is `diff oracle_other_case.tsv rust_other_case.tsv`.
    #[must_use]
    pub fn dump_other_case(&self) -> String {
        let mut out = String::new();
        for id in 0..self.reverse.len() as u32 {
            out.push_str(&id.to_string());
            out.push('\t');
            out.push_str(&self.get_other_case(id).to_string());
            out.push('\n');
        }
        out
    }

    /// Render the id→unichar table as `"<id>\t<unichar>\n"` lines — the exact
    /// shape the C++ oracle harness prints, so a byte-parity diff is
    /// `diff oracle_dump.tsv rust_dump.tsv`.
    #[must_use]
    pub fn dump(&self) -> String {
        let mut out = String::new();
        for (id, unichar) in self.reverse.iter().enumerate() {
            out.push_str(&id.to_string());
            out.push('\t');
            out.push_str(unichar);
            out.push('\n');
        }
        out
    }
}

/// A failure loading a `UNICHARSET`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UniCharSetError {
    /// The input had no count line.
    Empty,
    /// The count line was not a non-negative integer.
    BadCount,
    /// Fewer entry lines than the declared count.
    CountMismatch {
        /// The count declared on line 1.
        declared: usize,
        /// The number of entry lines actually found.
        found: usize,
    },
    /// The file could not be read (message from the underlying I/O error).
    Io(String),
}

impl std::fmt::Display for UniCharSetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => write!(f, "empty unicharset (no count line)"),
            Self::BadCount => write!(f, "first line is not a valid entry count"),
            Self::CountMismatch { declared, found } => {
                write!(f, "declared {declared} entries but found {found}")
            }
            Self::Io(msg) => write!(f, "unicharset read failed: {msg}"),
        }
    }
}

impl std::error::Error for UniCharSetError {}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
3
a 3 0,255,0,255,0,255,0,255,0,255 0 a Left a a
b 3 0,255,0,255,0,255,0,255,0,255 0 b Left b b
cd 5 0,255,0,255,0,255,0,255,0,255 0 cd Left cd cd
";

    #[test]
    fn parses_count_and_first_token_per_line() {
        let u = UniCharSet::load_from_str(SAMPLE).expect("valid");
        assert_eq!(u.size(), 3);
        assert_eq!(u.id_to_unichar(0), Some("a"));
        assert_eq!(u.id_to_unichar(2), Some("cd")); // multi-char unichar token
        assert_eq!(u.id_to_unichar(3), None); // out of range
    }

    #[test]
    fn bijection_round_trips() {
        let u = UniCharSet::load_from_str(SAMPLE).expect("valid");
        for id in 0..u.size() as u32 {
            let s = u.id_to_unichar(id).unwrap();
            assert_eq!(u.unichar_to_id(s), Some(id), "id {id} must round-trip");
        }
        assert_eq!(u.unichar_to_id("zzz"), None, "absent unichar -> None");
    }

    #[test]
    fn dump_matches_oracle_line_shape() {
        let u = UniCharSet::load_from_str(SAMPLE).expect("valid");
        assert_eq!(u.dump(), "0\ta\n1\tb\n2\tcd\n");
    }

    /// Tesseract stores the space unichar as the literal `"NULL"` token; load
    /// remaps it to `" "` (`unicharset.cpp:882`). This is the sole id-0
    /// discrepancy the byte-parity probe found against the C++ oracle on the
    /// real `eng.lstm-unicharset`.
    #[test]
    fn null_token_maps_to_space() {
        let u = UniCharSet::load_from_str("1\nNULL 0 Common 0\n").expect("valid");
        assert_eq!(u.id_to_unichar(0), Some(" "));
        assert_eq!(u.unichar_to_id(" "), Some(0));
        assert_eq!(
            u.unichar_to_id("NULL"),
            None,
            "NULL is the file token, never the runtime unichar"
        );
    }

    /// A sample exercising each property mask via the hex second column:
    /// `0x3`=alpha+lower, `0x5`=alpha+upper, `0x8`=digit, `0x10`=punct, `0x1`=alpha.
    const PROPS_SAMPLE: &str = "\
5
a 3 0,255,0,255,0,0,0,0,0,0 Latin 0 0 0 a
A 5 0,255,0,255,0,0,0,0,0,0 Latin 0 0 0 A
7 8 0,255,0,255,0,0,0,0,0,0 Common 0 0 0 7
. 10 0,255,0,255,0,0,0,0,0,0 Common 0 0 0 .
x 1 0,255,0,255,0,0,0,0,0,0 Latin 0 0 0 x
";

    #[test]
    fn properties_decode_from_hex_column() {
        let u = UniCharSet::load_from_str(PROPS_SAMPLE).expect("valid");
        // id 0 "a": 0x3 = ISALPHA | ISLOWER
        assert!(u.get_isalpha(0) && u.get_islower(0));
        assert!(!u.get_isupper(0) && !u.get_isdigit(0) && !u.get_ispunctuation(0));
        // id 1 "A": 0x5 = ISALPHA | ISUPPER
        assert!(u.get_isalpha(1) && u.get_isupper(1));
        assert!(!u.get_islower(1));
        // id 2 "7": 0x8 = ISDIGIT
        assert!(u.get_isdigit(2));
        assert!(!u.get_isalpha(2) && !u.get_ispunctuation(2));
        // id 3 ".": 0x10 = ISPUNCTUATION
        assert!(u.get_ispunctuation(3));
        assert!(!u.get_isalpha(3) && !u.get_isdigit(3));
        // id 4 "x": 0x1 = ISALPHA only
        assert!(u.get_isalpha(4));
        assert!(!u.get_islower(4) && !u.get_isupper(4));
    }

    /// The C++ accessor guards `INVALID_UNICHAR_ID` → `false`; an out-of-range id
    /// is the Rust analogue and must not panic.
    #[test]
    fn properties_out_of_range_is_false() {
        let u = UniCharSet::load_from_str(PROPS_SAMPLE).expect("valid");
        assert!(!u.get_isalpha(99));
        assert!(!u.get_islower(99));
        assert!(!u.get_isupper(99));
        assert!(!u.get_isdigit(99));
        assert!(!u.get_ispunctuation(99));
    }

    /// The plain-table loader always clears `isngram` (`unicharset.cpp:893`).
    #[test]
    fn isngram_always_false() {
        let u = UniCharSet::load_from_str(PROPS_SAMPLE).expect("valid");
        for id in 0..u.size() as u32 {
            assert!(!u.get_isngram(id));
        }
        assert!(!u.get_isngram(99));
    }

    #[test]
    fn dump_properties_matches_oracle_shape() {
        let u = UniCharSet::load_from_str(PROPS_SAMPLE).expect("valid");
        assert_eq!(
            u.dump_properties(),
            "0\t1 1 0 0 0\n1\t1 0 1 0 0\n2\t0 0 0 1 0\n3\t0 0 0 0 1\n4\t1 0 0 0 0\n"
        );
    }

    /// A missing or non-hex properties token defaults to "no properties" (the
    /// loader's documented tolerance for partial lines); the id↔unichar
    /// bijection is unaffected.
    #[test]
    fn missing_properties_token_defaults_to_zero() {
        let u = UniCharSet::load_from_str("2\na\nb 3\n").expect("valid");
        assert!(!u.get_isalpha(0)); // "a" has no second token -> 0
        assert!(u.get_isalpha(1) && u.get_islower(1)); // "b 3" -> 0x3
        assert_eq!(u.id_to_unichar(0), Some("a"));
    }

    /// Mirrors the real `eng.lstm-unicharset` mixed-tier shape: id 0 has no bbox
    /// CSV (script is token[2]); ids 1-3 do (script is token[3]). The interned
    /// table is `["NULL", "Common", "Latin"]` — exactly the oracle's table.
    const SCRIPT_SAMPLE: &str = "\
4
NULL 0 Common 0
a 3 0,255,0,255,0,0,0,0,0,0 Latin 1 0 1 a
b 3 0,255,0,255,0,0,0,0,0,0 Latin 1 0 1 b
5 8 0,255,0,255,0,0,0,0,0,0 Common 2 0 2 5
";

    #[test]
    fn scripts_intern_in_id_order_with_null_seeded_at_zero() {
        let u = UniCharSet::load_from_str(SCRIPT_SAMPLE).expect("valid");
        // null_script seeded at 0; real scripts follow in first-seen id order.
        assert_eq!(u.get_script_table_size(), 3);
        assert_eq!(u.script_from_script_id(0), Some("NULL"));
        assert_eq!(u.script_from_script_id(1), Some("Common")); // id 0's script
        assert_eq!(u.script_from_script_id(2), Some("Latin")); // id 1's script
        assert_eq!(u.script_from_script_id(3), None); // out of range
                                                      // per-id script ids (matches the libtesseract oracle on real eng shape)
        assert_eq!(u.get_script(0), 1); // Common
        assert_eq!(u.get_script(1), 2); // Latin (after the bbox CSV)
        assert_eq!(u.get_script(2), 2); // Latin (deduped)
        assert_eq!(u.get_script(3), 1); // Common (deduped)
    }

    #[test]
    fn script_out_of_range_is_null_sid() {
        let u = UniCharSet::load_from_str(SCRIPT_SAMPLE).expect("valid");
        assert_eq!(u.get_script(99), 0); // null_sid_
        assert_eq!(u.script_of(99), Some("NULL")); // resolves the null script
    }

    #[test]
    fn script_of_resolves_name() {
        let u = UniCharSet::load_from_str(SCRIPT_SAMPLE).expect("valid");
        assert_eq!(u.script_of(0), Some("Common"));
        assert_eq!(u.script_of(1), Some("Latin"));
        assert_eq!(u.script_of(3), Some("Common"));
    }

    #[test]
    fn dump_script_matches_oracle_shape() {
        let u = UniCharSet::load_from_str(SCRIPT_SAMPLE).expect("valid");
        assert_eq!(u.dump_script(), "0\t1\n1\t2\n2\t2\n3\t1\n");
    }

    /// id 0 `C` pairs with id 1 `c` and vice-versa; id 2 `.` has an out-of-range
    /// other_case (99 ≥ size 3) which clamps to itself (`unicharset.cpp:901`).
    const OTHER_CASE_SAMPLE: &str = "\
3
C 5 0,255,0,255,0,0,0,0,0,0 Latin 1 0 0 C
c 3 0,255,0,255,0,0,0,0,0,0 Latin 0 0 1 c
. 10 0,255,0,255,0,0,0,0,0,0 Common 99 0 2 .
";

    #[test]
    fn other_case_decodes_and_clamps() {
        let u = UniCharSet::load_from_str(OTHER_CASE_SAMPLE).expect("valid");
        assert_eq!(u.get_other_case(0), 1); // C -> c
        assert_eq!(u.get_other_case(1), 0); // c -> C
        assert_eq!(u.get_other_case(2), 2); // 99 >= size -> clamped to self
    }

    #[test]
    fn other_case_absent_defaults_to_self() {
        // A line with no other_case token (script only): default = size, clamps
        // to the id itself.
        let u = UniCharSet::load_from_str("1\nx 1 Latin\n").expect("valid");
        assert_eq!(u.get_other_case(0), 0);
    }

    #[test]
    fn other_case_out_of_range_id_is_invalid() {
        let u = UniCharSet::load_from_str(OTHER_CASE_SAMPLE).expect("valid");
        assert_eq!(u.get_other_case(99), -1); // INVALID_UNICHAR_ID
    }

    #[test]
    fn dump_other_case_matches_oracle_shape() {
        let u = UniCharSet::load_from_str(OTHER_CASE_SAMPLE).expect("valid");
        assert_eq!(u.dump_other_case(), "0\t1\n1\t0\n2\t2\n");
    }

    #[test]
    fn errors_are_typed() {
        assert_eq!(UniCharSet::load_from_str(""), Err(UniCharSetError::Empty));
        assert_eq!(
            UniCharSet::load_from_str("notanumber\n"),
            Err(UniCharSetError::BadCount)
        );
        assert_eq!(
            UniCharSet::load_from_str("5\na\nb\n"),
            Err(UniCharSetError::CountMismatch {
                declared: 5,
                found: 2
            })
        );
    }
}
