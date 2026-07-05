//! `UNICHARCOMPRESS` (the recoder) content store — the Rust side of the recoder
//! byte-parity leaf, sibling to [`crate::unicharset`].
//!
//! Tesseract's `UnicharCompress` (`ccutil/unicharcompress.{h,cpp}`) re-encodes
//! each unichar-id as a short sequence of small codes (Han radical-stroke,
//! Hangul Jamo, ligature dissection; pass-through for simple scripts). The LSTM
//! recognizer's output lattice speaks these **recoded codes, not raw
//! unichar-ids**, so `ids_to_text` only becomes real OCR output once the decode
//! table exists. Per the Core-First doctrine this is a **classid-keyed
//! content-store tier** (a loaded codec table — id ↔ code-sequence bijection +
//! bounds), exactly like [`crate::unicharset::UniCharSet`]: data-shaped, no
//! lifecycle vocabulary, no effects. It rides the existing keystone; it is NOT
//! IR-surface (`docs/OGAR-AS-IR.md` §3: adds no `Class` field, no `ActionDef`,
//! no `KausalSpec` slot).
//!
//! # Load-side scope
//!
//! This module transcodes the **load side only** — `DeSerialize` +
//! `EncodeUnichar` + `DecodeUnichar` + `code_range` (the recognizer runtime
//! surface). `ComputeEncoding` (the training-side table builder) is out of
//! scope. `SetupDecoder`'s full state — the `decoder_` map (code → id) **and**
//! the beam-search trie maps `is_valid_start_` / `final_codes_` / `next_codes_`
//! (`unicharcompress.cpp:396-434`) — is built here: they are computed table
//! state loaded alongside the encoder (data-shaped, no lifecycle), and the
//! recognizer's `RecodeBeamSearch` *consumes* them read-only via
//! [`UnicharCompress::is_valid_first_code`] / [`UnicharCompress::get_final_codes`]
//! / [`UnicharCompress::get_next_codes`] (the C++ `IsValidFirstCode` /
//! `GetFinalCodes` / `GetNextCodes` accessors). The maps are Core content; the
//! beam *search* that walks them is recognizer compute (Leaf 7b).
//!
//! # Binary format (byte-parity surface)
//!
//! Every prior leaf parsed text; the recoder is **binary** (`serialis.h` `TFile`
//! conventions). `UnicharCompress::Serialize` writes exactly the `encoder_`
//! vector (`unicharcompress.cpp:318-320`, comment `unicharcompress.h:229`: "the
//! only part that is serialized. The rest is computed on load"). The wire form
//! (little-endian; `TFile::swap_ == false` on x86) is:
//!
//! ```text
//! u32  count                         // TFile::DeSerialize(vector<T>), serialis.h:90
//! count × RecodedCharID:
//!   i8   self_normalized             // RecodedCharID::DeSerialize, unicharcompress.h:75
//!   i32  length                      // number of codes in use (<= kMaxCodeLen=9)
//!   i32 × length  code               // only `length` codes are written, not all 9
//! ```
//!
//! For real `eng.lstm-recoder` (112 pass-through entries, all length-1):
//! `4 + 112·(1+4+4) = 1012` bytes — the exact on-disk size, a first-principles
//! pre-registration of a correct parse. On load, `ComputeCodeRange`
//! (`unicharcompress.cpp:383`, `max(code)+1`) and the `decoder_` map
//! (`unicharcompress.cpp:400-402`, `decoder_[code]=id` in ascending-id order, so
//! **last writer wins** on a shared code) are recomputed.
//!
//! [`UnicharCompress::dump_encode`] / [`UnicharCompress::dump_decode`] are the
//! byte-parity surfaces, diffed against the C++ `UnicharCompress` oracle
//! (`recoder_oracle.cpp`, which links libtesseract, loads the same component via
//! `TFile`, and dumps `EncodeUnichar` / `DecodeUnichar` / `code_range`). The
//! oracle's `Encode∘Decode` round-trip + the `UNICHARSET` bijection guard the
//! 5.5.0-header / 5.3.4-lib ABI skew for this NEW object layout.
//!
//! # Strict-vs-lenient
//!
//! C++ `RecodedCharID::DeSerialize` reads `length` then reads that many `i32`
//! into the fixed `code_[9]` — a buffer overflow (UB) if `length > 9` on hostile
//! input. This reader instead rejects `length < 0 || length > kMaxCodeLen`
//! ([`RecoderError::BadCodeLength`]) and a truncated buffer
//! ([`RecoderError::UnexpectedEof`]). On well-formed trained data (`length` is
//! always 1..=3) the byte-parity diff is unaffected; the guard only fires on
//! corruption.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::Path;

/// `RecodedCharID::kMaxCodeLen` (tesseract `unicharcompress.h:35`) — the fixed
/// capacity of a code array. Hangul/Han use length 3; the array is sized 9.
const K_MAX_CODE_LEN: usize = 9;

/// The C++ `INVALID_UNICHAR_ID` sentinel (tesseract `unichar.h`) — what
/// [`UnicharCompress::decode`] returns for a code with no matching id, mirroring
/// `DecodeUnichar` (`unicharcompress.cpp:305-315`).
const INVALID_UNICHAR_ID: i32 = -1;

/// The `TFile::DeSerialize(vector<T>)` sanity cap (tesseract `serialis.h:96`):
/// a declared element count above this is treated as corrupt input.
const MAX_ELEMENTS: u32 = 50_000_000;

/// The code sequence for one recoded unichar-id — the transcription of
/// tesseract's `RecodedCharID` (`unicharcompress.h:32-109`).
///
/// Equality and hashing mirror the C++ `operator==` / `RecodedCharIDHash`
/// (`unicharcompress.h:79-99`): **only `length` + the used `code[0..length]`
/// participate**; `self_normalized` and any trailing array slots are ignored, so
/// this is a sound [`HashMap`] key for the decoder (`decoder_[code]`).
#[derive(Debug, Clone)]
pub struct RecodedCharId {
    /// True (`1`) if this is the master entry for ids sharing one code; stored as
    /// `i8` for serialization (`unicharcompress.h:104`). Preserved on load for
    /// round-trip fidelity; not part of identity.
    self_normalized: i8,
    /// The number of codes in use in `code` (`unicharcompress.h:106`).
    length: i32,
    /// The re-encoded form (`unicharcompress.h:108`). Only `code[0..length]` is
    /// meaningful; trailing slots are `0`.
    code: [i32; K_MAX_CODE_LEN],
}

impl Default for RecodedCharId {
    /// Mirrors the C++ default ctor (`unicharcompress.h:37`): `self_normalized =
    /// 1`, `length = 0`, all codes `0`.
    fn default() -> Self {
        Self {
            self_normalized: 1,
            length: 0,
            code: [0; K_MAX_CODE_LEN],
        }
    }
}

impl RecodedCharId {
    /// Construct a code from an explicit slice of code values — the beam-search
    /// consumer's key builder (the C++ `RecodedCharID::Set` loop,
    /// `unicharcompress.h:43`). `RecodeBeamSearch` builds a `prefix`
    /// (`codes[0..length]`) to query [`Self::get_final_codes`](UnicharCompress::get_final_codes)
    /// / [`get_next_codes`](UnicharCompress::get_next_codes) and a `full_code`
    /// (`prefix ++ code`) to feed [`UnicharCompress::decode`]. Only the first
    /// [`K_MAX_CODE_LEN`](struct@RecodedCharId) codes are kept; extras are dropped
    /// (the C++ fixed `code_[9]`). `self_normalized` is the default `1` (it never
    /// participates in identity).
    #[must_use]
    pub fn from_codes(codes: &[i32]) -> Self {
        let mut code = [0_i32; K_MAX_CODE_LEN];
        let len = codes.len().min(K_MAX_CODE_LEN);
        code[..len].copy_from_slice(&codes[..len]);
        Self {
            self_normalized: 1,
            length: len as i32,
            code,
        }
    }

    /// The codes in use — `code[0..length]`. The only bytes that carry identity.
    #[must_use]
    pub fn codes(&self) -> &[i32] {
        let len = self.length.max(0) as usize;
        // `length` is bounded to `<= K_MAX_CODE_LEN` at load; `min` keeps this
        // total even for a hand-built value.
        &self.code[..len.min(K_MAX_CODE_LEN)]
    }

    /// The number of codes in use (the C++ `length()`, `unicharcompress.h:62`).
    #[must_use]
    pub fn length(&self) -> i32 {
        self.length
    }

    /// Whether this code is empty (`length == 0`), the C++ `empty()`
    /// (`unicharcompress.h:58`).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.length == 0
    }

    /// Whether this is the self-normalizing master entry (`unicharcompress.h:104`).
    #[must_use]
    pub fn self_normalized(&self) -> bool {
        self.self_normalized != 0
    }

    /// The code value at `index` — the C++ `operator()(int)` (`unicharcompress.h:65`),
    /// reading `code_[index]` directly. Out-of-range indices read `0` (the array is
    /// zero-initialized and `SetupDecoder` only ever indexes `0..length`).
    #[must_use]
    pub fn code_at(&self, index: usize) -> i32 {
        self.code.get(index).copied().unwrap_or(0)
    }

    /// A copy truncated to `len` codes — the C++ `Truncate(int)`
    /// (`unicharcompress.h:40`, which only sets `length_`). The trailing `code`
    /// slots are retained but drop out of identity ([`codes`](Self::codes) /
    /// [`PartialEq`] / [`Hash`] read `code[0..length]`), exactly as C++ leaves
    /// `code_` intact and compares only `code_[0..length_]`.
    #[must_use]
    fn truncated(&self, len: i32) -> Self {
        let mut out = self.clone();
        out.length = len;
        out
    }

    /// Read one `RecodedCharID` from the little-endian cursor. Rejects a
    /// `length` outside `0..=kMaxCodeLen` (the C++ UB guard) and a short buffer.
    fn read(r: &mut ByteReader<'_>) -> Result<Self, RecoderError> {
        let self_normalized = r.read_i8()?;
        let length = r.read_i32()?;
        if length < 0 || length as usize > K_MAX_CODE_LEN {
            return Err(RecoderError::BadCodeLength(length));
        }
        let mut code = [0_i32; K_MAX_CODE_LEN];
        for slot in code.iter_mut().take(length as usize) {
            *slot = r.read_i32()?;
        }
        Ok(Self {
            self_normalized,
            length,
            code,
        })
    }
}

impl PartialEq for RecodedCharId {
    /// `operator==` (`unicharcompress.h:79-89`): compares `length` +
    /// `code[0..length]` only.
    fn eq(&self, other: &Self) -> bool {
        self.codes() == other.codes()
    }
}

impl Eq for RecodedCharId {}

impl Hash for RecodedCharId {
    /// Consistent with [`PartialEq`]: hash the used codes only. (The C++
    /// `RecodedCharIDHash` folds the same `code[0..length]`; the Rust hasher need
    /// only agree with `eq`, not reproduce the C++ bit-mix.)
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.codes().hash(state);
    }
}

/// A loaded `UnicharCompress` (the recoder): the `encoder_` table (id → codes),
/// its inverse `decoder_` (codes → id), and `code_range` — the transcription of
/// tesseract's `UnicharCompress` load side (`unicharcompress.{h,cpp}`).
#[derive(Debug, Clone, Default)]
pub struct UnicharCompress {
    /// id → code sequence (index IS the unichar-id). The only serialized part
    /// (`unicharcompress.h:229-230`).
    encoder: Vec<RecodedCharId>,
    /// code → unichar-id, recomputed on load (`SetupDecoder`,
    /// `unicharcompress.cpp:400-402`). Last-writer-wins on a shared code.
    decoder: HashMap<RecodedCharId, u32>,
    /// `is_valid_start_` (`unicharcompress.h:234`): indexed by code value in
    /// `0..code_range`, `true` where some entry's first code is that value — the
    /// beam search's valid-first-code gate (`IsValidFirstCode`).
    is_valid_start: Vec<bool>,
    /// `final_codes_` (`unicharcompress.h:241`): prefix (`code[0..len-1]`) → the
    /// last codes that complete a full sequence from that prefix. Keyed by the
    /// truncated [`RecodedCharId`]; the empty prefix maps every length-1 code.
    final_codes: HashMap<RecodedCharId, Vec<i32>>,
    /// `next_codes_` (`unicharcompress.h:237`): prefix → the valid *non-final*
    /// continuation codes. Empty for a pass-through recoder (all length-1); only
    /// multi-code scripts (Han/Hangul, length 3) populate it.
    next_codes: HashMap<RecodedCharId, Vec<i32>>,
    /// `1 + max code value` (`ComputeCodeRange`, `unicharcompress.cpp:383-393`);
    /// the lattice width. `0` for an empty encoder (`-1 + 1`).
    code_range: i32,
}

impl UnicharCompress {
    /// Load a recoder from the raw little-endian bytes of a `.lstm-recoder`
    /// component (the C++ `DeSerialize`, `unicharcompress.cpp:323-330`): read the
    /// `encoder_` vector, then recompute `code_range` and the decode map.
    ///
    /// # Errors
    ///
    /// [`RecoderError::UnexpectedEof`] on a truncated buffer,
    /// [`RecoderError::TooManyElements`] if the declared count exceeds the
    /// `serialis.h` sanity cap, and [`RecoderError::BadCodeLength`] if any entry
    /// declares a code length outside `0..=9`.
    pub fn from_le_bytes(bytes: &[u8]) -> Result<Self, RecoderError> {
        let mut r = ByteReader::new(bytes);
        let count = r.read_u32()?;
        if count > MAX_ELEMENTS {
            return Err(RecoderError::TooManyElements(count));
        }
        let mut encoder = Vec::with_capacity(count as usize);
        for _ in 0..count {
            encoder.push(RecodedCharId::read(&mut r)?);
        }
        // Trailing bytes are ignored on purpose: a component extracted from a
        // TFile stream may be followed by the next component's bytes (the C++
        // reader leaves the cursor for them). A standalone `.lstm-recoder` is
        // consumed exactly.
        let mut this = Self {
            encoder,
            decoder: HashMap::new(),
            is_valid_start: Vec::new(),
            final_codes: HashMap::new(),
            next_codes: HashMap::new(),
            code_range: 0,
        };
        this.compute_code_range();
        this.setup_decoder();
        Ok(this)
    }

    /// Load a recoder from a `.lstm-recoder` file (a thin wrapper over
    /// [`Self::from_le_bytes`]). Extract one via
    /// `combine_tessdata -u eng.traineddata /tmp/eng.`.
    ///
    /// # Errors
    ///
    /// [`RecoderError::Io`] if the file cannot be read, else the parse errors of
    /// [`Self::from_le_bytes`].
    pub fn load_from_file(path: &Path) -> Result<Self, RecoderError> {
        let bytes = std::fs::read(path).map_err(|e| RecoderError::Io(e.to_string()))?;
        Self::from_le_bytes(&bytes)
    }

    /// `1 + max code value` — the lattice width (`code_range`,
    /// `unicharcompress.h:171`).
    #[must_use]
    pub fn code_range(&self) -> i32 {
        self.code_range
    }

    /// The number of encoded unichar-ids (`encoder_.size()`).
    #[must_use]
    pub fn len(&self) -> usize {
        self.encoder.len()
    }

    /// Whether the encoder is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.encoder.is_empty()
    }

    /// The code sequence for `unichar_id`, or `None` if out of range — the C++
    /// `EncodeUnichar` (`unicharcompress.cpp:295-301`; a `None` here is the C++
    /// return of length `0`).
    #[must_use]
    pub fn encode(&self, unichar_id: u32) -> Option<&RecodedCharId> {
        self.encoder.get(unichar_id as usize)
    }

    /// The unichar-id for `code`, or [`INVALID_UNICHAR_ID`] (`-1`) if the code is
    /// ill-formed or unknown — the C++ `DecodeUnichar`
    /// (`unicharcompress.cpp:305-315`).
    #[must_use]
    pub fn decode(&self, code: &RecodedCharId) -> i32 {
        let len = code.length();
        if len <= 0 || len as usize > K_MAX_CODE_LEN {
            return INVALID_UNICHAR_ID;
        }
        self.decoder
            .get(code)
            .map_or(INVALID_UNICHAR_ID, |&id| id as i32)
    }

    /// `ComputeCodeRange` (`unicharcompress.cpp:383-393`): `code_range = 1 + max`
    /// code value over every position of every entry (`0` for an empty encoder).
    fn compute_code_range(&mut self) {
        let mut max = -1_i32;
        for entry in &self.encoder {
            for &c in entry.codes() {
                if c > max {
                    max = c;
                }
            }
        }
        self.code_range = max + 1;
    }

    /// The full `SetupDecoder` (`unicharcompress.cpp:395-436`): in one ascending-id
    /// pass over `encoder_`, build the `decoder_` map (code → id, **last writer
    /// wins** on a shared code) **and** the beam-search trie maps —
    /// `is_valid_start_` (`code(0)` is a valid first code), `final_codes_` (prefix
    /// `code[0..len-1]` → the completing last codes), and `next_codes_` (prefix →
    /// valid non-final continuations). The `while (--len >= 0)` prefix walk climbs
    /// from the direct parent toward the empty prefix, stopping at the first
    /// already-populated `next_codes_` entry (that prefix, and all shorter ones,
    /// were seeded by an earlier entry).
    ///
    /// For the eng.lstm pass-through recoder (112 entries, all length-1) this is
    /// `is_valid_start_[c0]=true` for each, `final_codes_[<empty>]` = every
    /// distinct `code(0)` in id order, and an empty `next_codes_` (the `--len`
    /// loop never runs). Multi-code scripts (Han/Hangul, length 3) exercise the
    /// full trie.
    fn setup_decoder(&mut self) {
        self.decoder.clear();
        self.decoder.reserve(self.encoder.len());
        self.is_valid_start = vec![false; self.code_range.max(0) as usize];
        self.final_codes.clear();
        self.next_codes.clear();
        // Iterate by index so the loop body can mutate the other maps without
        // holding a borrow of `self.encoder` (C++ reads `encoder_[c]` by value).
        for id in 0..self.encoder.len() {
            let code = self.encoder[id].clone();
            self.decoder.insert(code.clone(), id as u32);
            let length = code.length();
            if length <= 0 {
                // Trained recoders never carry an empty entry (the reader rejects
                // `length < 0`, and `ComputeEncoding` emits length >= 1); the C++
                // would degenerately index `is_valid_start_[code(0)=0]`. Skipping
                // it cannot affect the byte-parity diff on real data.
                continue;
            }
            let last = length - 1; // index of the final code, `code.length() - 1`
            if let Some(slot) = self.is_valid_start.get_mut(code.code_at(0) as usize) {
                *slot = true;
            }
            let prefix = code.truncated(last);
            if let Some(list) = self.final_codes.get_mut(&prefix) {
                let v = code.code_at(last as usize);
                if !list.contains(&v) {
                    list.push(v);
                }
            } else {
                self.final_codes
                    .insert(prefix, vec![code.code_at(last as usize)]);
                let mut len = last;
                loop {
                    len -= 1;
                    if len < 0 {
                        break;
                    }
                    let p = code.truncated(len);
                    let v = code.code_at(len as usize);
                    if let Some(list) = self.next_codes.get_mut(&p) {
                        // Reached via multiple code lengths: dedup, then stop —
                        // this prefix (and all shorter) is already seeded.
                        if !list.contains(&v) {
                            list.push(v);
                        }
                        break;
                    }
                    self.next_codes.insert(p, vec![v]);
                }
            }
        }
    }

    /// Whether `code` is a valid start (or single) code — the C++
    /// `IsValidFirstCode` (`unicharcompress.h:182`). Bounds-checked (C++ indexes
    /// `is_valid_start_[code]` unchecked); an out-of-range code is not valid.
    #[must_use]
    pub fn is_valid_first_code(&self, code: i32) -> bool {
        usize::try_from(code)
            .ok()
            .and_then(|i| self.is_valid_start.get(i))
            .copied()
            .unwrap_or(false)
    }

    /// The valid final codes that complete a sequence from `prefix`, or `None` —
    /// the C++ `GetFinalCodes` (`unicharcompress.h:193`). `prefix` is a
    /// [`RecodedCharId`] truncated to the codes seen so far.
    #[must_use]
    pub fn get_final_codes(&self, prefix: &RecodedCharId) -> Option<&[i32]> {
        self.final_codes.get(prefix).map(Vec::as_slice)
    }

    /// The valid non-final continuation codes for `prefix`, or `None` — the C++
    /// `GetNextCodes` (`unicharcompress.h:187`).
    #[must_use]
    pub fn get_next_codes(&self, prefix: &RecodedCharId) -> Option<&[i32]> {
        self.next_codes.get(prefix).map(Vec::as_slice)
    }

    /// Render the id→code table as `"<id>\t<len>\t<c0>[,<c1>...]\n"` lines — the
    /// exact shape the C++ recoder oracle's `encode` mode prints, so the
    /// byte-parity diff is `diff oracle_recoder_encode.tsv rust_recoder_encode.tsv`.
    #[must_use]
    pub fn dump_encode(&self) -> String {
        let mut out = String::new();
        for (id, entry) in self.encoder.iter().enumerate() {
            out.push_str(&id.to_string());
            out.push('\t');
            out.push_str(&entry.length().to_string());
            out.push('\t');
            for (i, &c) in entry.codes().iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&c.to_string());
            }
            out.push('\n');
        }
        out
    }

    /// Render `"code_range\t<N>\n"` then `"<id>\t<decoded>\n"` lines (where
    /// `decoded = decode(encode(id))`) — the exact shape the C++ recoder oracle's
    /// `decode` mode prints, so the byte-parity diff is
    /// `diff oracle_recoder_decode.tsv rust_recoder_decode.tsv`. On a shared code
    /// the decoded id is the last-writer, matching the C++ map.
    #[must_use]
    pub fn dump_decode(&self) -> String {
        let mut out = String::new();
        out.push_str("code_range\t");
        out.push_str(&self.code_range.to_string());
        out.push('\n');
        for (id, entry) in self.encoder.iter().enumerate() {
            out.push_str(&id.to_string());
            out.push('\t');
            out.push_str(&self.decode(entry).to_string());
            out.push('\n');
        }
        out
    }

    /// Render the beam-search maps in a **deterministic order** (the C++
    /// `unordered_map` iteration order is unspecified and differs from Rust's
    /// [`HashMap`], so the dump drives itself off `encoder_` id-order instead):
    ///
    /// ```text
    /// is_valid_start\t<code_range>
    /// <code>\t<0|1>                       // for each code in 0..code_range
    /// final\t<prefix csv>\t<final codes csv | ->   // each distinct prefix, once
    /// next\t<prefix csv>\t<next codes csv | ->
    /// ```
    ///
    /// Distinct prefixes are enumerated by walking every entry in id order and,
    /// within each, truncation lengths `0..length` ascending, emitting each prefix
    /// the first time it is seen. The C++ oracle's `beam` mode performs the
    /// identical walk via `GetFinalCodes` / `GetNextCodes`, so the diff is
    /// `diff oracle_recoder_beam.tsv rust_recoder_beam.tsv`. The per-prefix code
    /// lists are already in push order (id-ascending, deduped) on both sides.
    #[must_use]
    pub fn dump_beam(&self) -> String {
        fn csv(codes: &[i32]) -> String {
            let mut s = String::new();
            for (i, c) in codes.iter().enumerate() {
                if i > 0 {
                    s.push(',');
                }
                s.push_str(&c.to_string());
            }
            s
        }
        fn list_or_dash(list: Option<&[i32]>) -> String {
            match list {
                Some(l) => csv(l),
                None => "-".to_string(),
            }
        }

        let mut out = String::new();
        out.push_str("is_valid_start\t");
        out.push_str(&self.code_range.to_string());
        out.push('\n');
        for (i, &valid) in self.is_valid_start.iter().enumerate() {
            out.push_str(&i.to_string());
            out.push('\t');
            out.push(if valid { '1' } else { '0' });
            out.push('\n');
        }

        let mut seen: std::collections::HashSet<RecodedCharId> = std::collections::HashSet::new();
        for code in &self.encoder {
            for l in 0..code.length().max(0) {
                let prefix = code.truncated(l);
                if !seen.insert(prefix.clone()) {
                    continue;
                }
                out.push_str("final\t");
                out.push_str(&csv(prefix.codes()));
                out.push('\t');
                out.push_str(&list_or_dash(self.get_final_codes(&prefix)));
                out.push('\n');
                out.push_str("next\t");
                out.push_str(&csv(prefix.codes()));
                out.push('\t');
                out.push_str(&list_or_dash(self.get_next_codes(&prefix)));
                out.push('\n');
            }
        }
        out
    }
}

/// A little-endian byte cursor over the recoder component — the reader half of
/// the `TFile` primitives this leaf needs (`FReadEndian` with `swap_ == false`).
struct ByteReader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> ByteReader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    /// Advance over `n` bytes, or [`RecoderError::UnexpectedEof`] if short.
    fn take(&mut self, n: usize) -> Result<&'a [u8], RecoderError> {
        let end = self.pos.checked_add(n).ok_or(RecoderError::UnexpectedEof)?;
        let slice = self
            .bytes
            .get(self.pos..end)
            .ok_or(RecoderError::UnexpectedEof)?;
        self.pos = end;
        Ok(slice)
    }

    fn read_i8(&mut self) -> Result<i8, RecoderError> {
        Ok(self.take(1)?[0] as i8)
    }

    fn read_u32(&mut self) -> Result<u32, RecoderError> {
        let arr: [u8; 4] = self
            .take(4)?
            .try_into()
            .map_err(|_| RecoderError::UnexpectedEof)?;
        Ok(u32::from_le_bytes(arr))
    }

    fn read_i32(&mut self) -> Result<i32, RecoderError> {
        let arr: [u8; 4] = self
            .take(4)?
            .try_into()
            .map_err(|_| RecoderError::UnexpectedEof)?;
        Ok(i32::from_le_bytes(arr))
    }
}

/// A failure loading a `UnicharCompress` (recoder).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoderError {
    /// The buffer ended mid-field.
    UnexpectedEof,
    /// The declared element count exceeded the `serialis.h` sanity cap.
    TooManyElements(u32),
    /// A `RecodedCharID` declared a code length outside `0..=9` (the C++ fixed
    /// array capacity `kMaxCodeLen`).
    BadCodeLength(i32),
    /// The file could not be read (message from the underlying I/O error).
    Io(String),
}

impl std::fmt::Display for RecoderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnexpectedEof => write!(f, "recoder buffer ended mid-field"),
            Self::TooManyElements(n) => {
                write!(
                    f,
                    "recoder declared {n} elements (over the {MAX_ELEMENTS} cap)"
                )
            }
            Self::BadCodeLength(len) => {
                write!(
                    f,
                    "recoded code length {len} out of range 0..={K_MAX_CODE_LEN}"
                )
            }
            Self::Io(msg) => write!(f, "recoder read failed: {msg}"),
        }
    }
}

impl std::error::Error for RecoderError {}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a `.lstm-recoder` byte buffer from `(self_normalized, codes)`
    /// entries, in the exact little-endian wire form the C++ `Serialize` writes.
    fn build(entries: &[(i8, &[i32])]) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(&u32::try_from(entries.len()).unwrap().to_le_bytes());
        for (self_norm, codes) in entries {
            b.push(*self_norm as u8);
            b.extend_from_slice(&i32::try_from(codes.len()).unwrap().to_le_bytes());
            for &c in *codes {
                b.extend_from_slice(&c.to_le_bytes());
            }
        }
        b
    }

    #[test]
    fn parses_count_and_entries() {
        let bytes = build(&[(1, &[0]), (1, &[5]), (1, &[5])]);
        let rec = UnicharCompress::from_le_bytes(&bytes).expect("valid");
        assert_eq!(rec.len(), 3);
        assert_eq!(rec.encode(0).unwrap().codes(), &[0]);
        assert_eq!(rec.encode(2).unwrap().codes(), &[5]);
        assert!(rec.encode(3).is_none(), "out-of-range id -> None");
    }

    #[test]
    fn code_range_is_max_plus_one() {
        // max code value 5 -> code_range 6.
        let rec = UnicharCompress::from_le_bytes(&build(&[(1, &[0]), (1, &[5]), (1, &[3])]))
            .expect("valid");
        assert_eq!(rec.code_range(), 6);
        // Empty encoder -> -1 + 1 = 0 (matches ComputeCodeRange's seed).
        let empty = UnicharCompress::from_le_bytes(&build(&[])).expect("valid");
        assert_eq!(empty.code_range(), 0);
    }

    #[test]
    fn decode_is_last_writer_wins_on_shared_code() {
        // ids 1 and 2 both encode to code [5]; decoder keeps the last (id 2) —
        // exactly the eng.lstm-recoder id1/id2 -> code 110 case.
        let rec = UnicharCompress::from_le_bytes(&build(&[(1, &[0]), (1, &[5]), (1, &[5])]))
            .expect("valid");
        assert_eq!(rec.decode(rec.encode(0).unwrap()), 0);
        assert_eq!(
            rec.decode(rec.encode(1).unwrap()),
            2,
            "shared code -> last id"
        );
        assert_eq!(rec.decode(rec.encode(2).unwrap()), 2);
    }

    #[test]
    fn decode_unknown_or_illformed_is_invalid() {
        let rec = UnicharCompress::from_le_bytes(&build(&[(1, &[0])])).expect("valid");
        // An empty code (length 0) is ill-formed for decode.
        assert_eq!(rec.decode(&RecodedCharId::default()), INVALID_UNICHAR_ID);
    }

    #[test]
    fn equality_ignores_self_normalized_and_trailing() {
        // Same code, different self_normalized -> equal (C++ operator==).
        let a = UnicharCompress::from_le_bytes(&build(&[(1, &[7])])).expect("valid");
        let b = UnicharCompress::from_le_bytes(&build(&[(0, &[7])])).expect("valid");
        assert_eq!(a.encode(0).unwrap(), b.encode(0).unwrap());
    }

    #[test]
    fn dump_encode_matches_oracle_shape() {
        // A multi-code entry exercises the comma join.
        let rec = UnicharCompress::from_le_bytes(&build(&[(1, &[0]), (1, &[5]), (1, &[1, 2, 3])]))
            .expect("valid");
        assert_eq!(rec.dump_encode(), "0\t1\t0\n1\t1\t5\n2\t3\t1,2,3\n");
    }

    #[test]
    fn dump_decode_matches_oracle_shape() {
        let rec = UnicharCompress::from_le_bytes(&build(&[(1, &[0]), (1, &[5]), (1, &[5])]))
            .expect("valid");
        // code_range = 6; id1 decodes to 2 (last-writer on shared code [5]).
        assert_eq!(rec.dump_decode(), "code_range\t6\n0\t0\n1\t2\n2\t2\n");
    }

    /// Build a `RecodedCharId` from a slice of codes — a beam-map query key.
    fn rc(codes: &[i32]) -> RecodedCharId {
        let mut code = [0_i32; K_MAX_CODE_LEN];
        code[..codes.len()].copy_from_slice(codes);
        RecodedCharId {
            self_normalized: 1,
            length: codes.len() as i32,
            code,
        }
    }

    #[test]
    fn from_codes_builds_identity_key() {
        // The public beam-consumer constructor agrees with the private test `rc`
        // (identity = length + code[0..length]); an empty slice is the empty
        // prefix (== default); overflow past kMaxCodeLen is truncated.
        assert_eq!(RecodedCharId::from_codes(&[2, 3]), rc(&[2, 3]));
        assert_eq!(RecodedCharId::from_codes(&[2, 3]).codes(), &[2, 3]);
        assert_eq!(RecodedCharId::from_codes(&[]), RecodedCharId::default());
        assert_eq!(RecodedCharId::from_codes(&[7]).length(), 1);
        let over = RecodedCharId::from_codes(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]);
        assert_eq!(over.length() as usize, K_MAX_CODE_LEN, "truncated to 9");
        assert_eq!(over.codes(), &[1, 2, 3, 4, 5, 6, 7, 8, 9]);
    }

    #[test]
    fn beam_maps_passthrough_all_length1() {
        // 3 pass-through codes: every code is a valid start, the empty prefix maps
        // all three final codes, and next_codes stays empty (the --len loop never
        // runs) — the eng.lstm shape in miniature.
        let rec =
            UnicharCompress::from_le_bytes(&build(&[(1, &[0]), (1, &[1]), (1, &[2])])).expect("ok");
        assert!(
            rec.is_valid_first_code(0) && rec.is_valid_first_code(1) && rec.is_valid_first_code(2)
        );
        assert!(
            !rec.is_valid_first_code(3),
            "out-of-range code is not a start"
        );
        assert_eq!(
            rec.get_final_codes(&RecodedCharId::default()),
            Some(&[0, 1, 2][..])
        );
        assert_eq!(rec.get_next_codes(&RecodedCharId::default()), None);
        assert_eq!(
            rec.dump_beam(),
            "is_valid_start\t3\n0\t1\n1\t1\n2\t1\nfinal\t\t0,1,2\nnext\t\t-\n"
        );
    }

    #[test]
    fn beam_maps_trie_multicode() {
        // Length-3 (Han/Hangul-shaped) entries sharing prefixes exercise the full
        // trie: the `while (--len >= 0)` walk, the dedup-then-`break` on an already
        // seeded next prefix, and multiple finals under one prefix.
        //   id0 [1]      id1 [2,3,4]   id2 [2,3,5]   id3 [2,6,7]
        let rec = UnicharCompress::from_le_bytes(&build(&[
            (1, &[1]),
            (1, &[2, 3, 4]),
            (1, &[2, 3, 5]),
            (1, &[2, 6, 7]),
        ]))
        .expect("ok");
        // final_codes: {} <- [1]; [2,3] <- [4,5]; [2,6] <- [7]
        assert_eq!(
            rec.get_final_codes(&RecodedCharId::default()),
            Some(&[1][..])
        );
        assert_eq!(rec.get_final_codes(&rc(&[2, 3])), Some(&[4, 5][..]));
        assert_eq!(rec.get_final_codes(&rc(&[2, 6])), Some(&[7][..]));
        assert_eq!(
            rec.get_final_codes(&rc(&[2])),
            None,
            "[2] is a next-prefix, not final"
        );
        // next_codes: {} <- [2] (from id1 only); [2] <- [3,6] (id1 seeds 3, id3 adds 6 then breaks)
        assert_eq!(
            rec.get_next_codes(&RecodedCharId::default()),
            Some(&[2][..])
        );
        assert_eq!(rec.get_next_codes(&rc(&[2])), Some(&[3, 6][..]));
        // is_valid_start: only the first codes 1 and 2 (code_range = 7+1 = 8).
        assert!(rec.is_valid_first_code(1) && rec.is_valid_first_code(2));
        assert!(!rec.is_valid_first_code(3) && !rec.is_valid_first_code(0));
        assert_eq!(
            rec.dump_beam(),
            "is_valid_start\t8\n0\t0\n1\t1\n2\t1\n3\t0\n4\t0\n5\t0\n6\t0\n7\t0\n\
             final\t\t1\nnext\t\t2\n\
             final\t2\t-\nnext\t2\t3,6\n\
             final\t2,3\t4,5\nnext\t2,3\t-\n\
             final\t2,6\t7\nnext\t2,6\t-\n"
        );
    }

    #[test]
    fn truncated_buffer_errors() {
        let mut bytes = build(&[(1, &[0])]);
        bytes.pop(); // drop the last code byte
        assert_eq!(
            UnicharCompress::from_le_bytes(&bytes).unwrap_err(),
            RecoderError::UnexpectedEof
        );
        // A count with no entries at all.
        assert_eq!(
            UnicharCompress::from_le_bytes(&[3, 0, 0, 0]).unwrap_err(),
            RecoderError::UnexpectedEof
        );
    }

    #[test]
    fn bad_code_length_errors() {
        // count=1, self_norm=1, length=10 (> kMaxCodeLen) — the C++ UB case.
        let mut bytes = vec![1, 0, 0, 0, 1];
        bytes.extend_from_slice(&10_i32.to_le_bytes());
        assert_eq!(
            UnicharCompress::from_le_bytes(&bytes).unwrap_err(),
            RecoderError::BadCodeLength(10)
        );
    }

    #[test]
    fn too_many_elements_errors() {
        // A declared count over the cap fails fast without allocating.
        let bytes = (MAX_ELEMENTS + 1).to_le_bytes();
        assert_eq!(
            UnicharCompress::from_le_bytes(&bytes).unwrap_err(),
            RecoderError::TooManyElements(MAX_ELEMENTS + 1)
        );
    }
}
