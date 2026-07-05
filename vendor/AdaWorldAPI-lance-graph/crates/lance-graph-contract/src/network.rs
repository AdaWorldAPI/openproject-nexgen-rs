//! LSTM `Network` layer-graph structure — the Rust side of the network
//! base-header byte-parity leaf, and the **sink of the ruff→OGAR harvest onto
//! the V3 SoA** ([`crate::facet::FacetCascade`]).
//!
//! Tesseract's recognizer is a tree of `Network` subclasses (`lstm/network.{h,cpp}`
//! + `series.cpp` / `parallel.cpp` / `fullyconnected.cpp` / `lstm.cpp` / …). Every
//! node — whatever its subclass — is serialized with the SAME base header, written
//! by `Network::Serialize` and read back by the factory `Network::CreateFromFile`
//! (`network.cpp:155-248`). This module transcodes that **base header** (the shared
//! prefix of every layer) + the `kTypeNames` on-wire type discriminant, and sinks
//! each parsed node onto a content-blind [`FacetCascade`] — the operator's
//! "16-byte tenant, classid + 12 bytes" V3 substrate.
//!
//! # Core-First placement
//!
//! Per the Core-First doctrine this is **structure** (identity + typed dims), not
//! compute: the recognizer's `Forward`/weight math lives in `tesseract-recognizer`
//! (deps ndarray); the layer *graph* — which types, nested how, with what
//! `ni`/`no` — is content the OGAR Core owns, exactly like the recoder
//! ([`crate::unicharcompress`]) and the unicharset ([`crate::unicharset`]). The
//! `ruff_cpp_spo` harvest (`has_function` / `virtually_overrides`) is the
//! `classid → ClassView` method-resolution manifest; THIS is where a harvested
//! node lands as a typed SoA row. No parallel object model: a network node is a
//! [`FacetCascade`], its type a `classid`, never a bespoke `enum NetworkKind`.
//!
//! # Base-header wire format (byte-parity surface)
//!
//! The factory reads, in order (`network.cpp:214-248`; little-endian,
//! `TFile::swap_ == false` on x86; `std::string` = `u32 len` + `len` raw bytes,
//! `serialis.cpp:94-110`):
//!
//! ```text
//! i8   tag                 // always NT_NONE(0); getNetworkType, network.cpp:191
//! u32  type_name_len       // then the ASCII type name (kTypeNames entry)
//! …    type_name bytes     // "Series" / "Input" / "LSTM" / … — the discriminant
//! i8   training            // TrainingState (recognizer = TS_DISABLED)
//! i8   needs_to_backprop   // 0/1
//! i32  network_flags       // NetworkFlags bits
//! i32  ni                  // number of inputs
//! i32  no                  // number of outputs
//! i32  num_weights         // weights in THIS node and its sub-network (cumulative)
//! u32  name_len            // then the layer's unique name
//! …    name bytes
//! ```
//!
//! then the subclass's own `DeSerialize` payload (weights / children) — DEFERRED
//! to follow-up leaves (the per-subclass payloads: `Plumbing` reads its child
//! vector, `FullyConnected`/`LSTM` read `WeightMatrix` blobs). This leaf proves the
//! shared base header, exactly as the recoder leaf proved the header before the
//! beam maps.
//!
//! For real `eng.lstm` (the extracted `lstm` component; `LSTMRecognizer::DeSerialize`
//! calls `Network::CreateFromFile` FIRST, `lstmrecognizer.cpp:135`) the outermost
//! node parses to `type=Series, ni=36, no=111, num_weights=385807` — matching the
//! model spec `[1,36,0,1[C3,3Ft16]Mp3,3TxyLfys48Lfx96RxLrx96Lfx192Fc111]` (ni=36
//! feature rows, no=111 = the Fc111 softmax classes). That is the first-principles
//! pre-registration of a correct parse (the recoder-leaf method).
//!
//! [`NetworkHeader::dump`] is the byte-parity surface, diffed against the C++
//! `network_spec_oracle` (which links libtesseract, calls the real
//! `Network::CreateFromFile`, and dumps `spec()` / `ni()` / `no()` /
//! `num_weights()` / `name()` of the loaded top node).

use crate::facet::{FacetCascade, FacetTier};
use crate::ogar_codebook::compose_classid;

/// The `network_layer` container concept in the `0x08XX` OCR domain
/// ([`crate::ogar_codebook`]). One canon-high slot for the KIND "a Tesseract
/// network layer"; the SPECIFIC subclass (Series / LSTM / …) is the classid's
/// custom-low half = the [`NetworkType`] ordinal, NOT 27 codebook slots (the
/// "container kinds, not content" mint discipline). `compose_classid(NETWORK_LAYER,
/// nt as u16)` is the node's `facet_classid`.
///
/// **Custom-half invariant:** a network-layer classid's custom-low half is the
/// [`NetworkType`] ordinal — a recognizer-INTERNAL facet discriminant, never a
/// render/RBAC app-prefix ([`classid_app_prefix`](crate::ogar_codebook::classid_app_prefix)).
/// These facet classids stay inside the OCR recognizer's SoA; they are never fed
/// to the app-prefix render path (which would misread ordinal 14 as an `AppPrefix`).
/// The value is kept in lock-step with the codebook by
/// [`tests::network_layer_const_matches_codebook`].
pub const NETWORK_LAYER: u16 = 0x0804;

/// `NetworkType` — the serialized layer-type discriminant (`network.h:41-78`,
/// `enum NetworkType`). The ordinal IS the discriminant and is stable across
/// versions (the `kTypeNames` string, written on the wire, decouples the on-disk
/// form from the enum order — `network.cpp:56-75`). `NT_NONE`(0) is the naked base
/// class / "invalid" sentinel; `NT_COUNT` is the array size, not a real type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum NetworkType {
    /// The naked base class ("Invalid" on the wire) — the 0 sentinel.
    None = 0,
    /// Inputs from an image.
    Input = 1,
    /// Duplicates inputs in a sliding-window neighborhood.
    Convolve = 2,
    /// Chooses the max result from a rectangle.
    Maxpool = 3,
    /// Runs networks in parallel.
    Parallel = 4,
    /// Runs identical networks in parallel.
    Replicated = 5,
    /// Runs LTR and RTL LSTMs in parallel.
    ParRlLstm = 6,
    /// Runs Up and Down LSTMs in parallel.
    ParUdLstm = 7,
    /// Runs 4 LSTMs in parallel.
    Par2dLstm = 8,
    /// Executes a sequence of layers.
    Series = 9,
    /// Scales the time/y size but makes the output deeper.
    Reconfig = 10,
    /// Reverses the x direction of the inputs/outputs.
    XReversed = 11,
    /// Reverses the y-direction of the inputs/outputs.
    YReversed = 12,
    /// Transposes x and y (for a single op).
    XyTranspose = 13,
    /// Long-Short-Term-Memory block.
    Lstm = 14,
    /// LSTM that only keeps its last output.
    LstmSummary = 15,
    /// Fully connected logistic nonlinearity.
    Logistic = 16,
    /// Fully connected rect-lin version of logistic.
    PosClip = 17,
    /// Fully connected rect-lin version of tanh.
    SymClip = 18,
    /// Fully connected with tanh nonlinearity.
    Tanh = 19,
    /// Fully connected with rectifier nonlinearity.
    Relu = 20,
    /// Fully connected with no nonlinearity.
    Linear = 21,
    /// Softmax with exponential normalization, with CTC.
    Softmax = 22,
    /// Softmax with exponential normalization, no CTC.
    SoftmaxNoCtc = 23,
    /// 1-d LSTM with built-in fully connected softmax.
    LstmSoftmax = 24,
    /// 1-d LSTM with built-in binary-encoded softmax.
    LstmSoftmaxEncoded = 25,
    /// A TensorFlow graph encapsulated as a Tesseract network.
    TensorFlow = 26,
}

/// The number of real `NetworkType`s (`NT_COUNT`, `network.h:78`) — the length of
/// the [`NetworkType::TYPE_NAMES`] table.
pub const NT_COUNT: usize = 27;

impl NetworkType {
    /// The on-wire `kTypeNames` strings (`network.cpp:60-75`), indexed by ordinal.
    /// This is the serialization discriminant matched by `getNetworkType`
    /// (`network.cpp:191-209`) — index-aligned with the enum, so
    /// `TYPE_NAMES[nt as usize] == nt.type_name()`.
    pub const TYPE_NAMES: [&'static str; NT_COUNT] = [
        "Invalid",
        "Input",
        "Convolve",
        "Maxpool",
        "Parallel",
        "Replicated",
        "ParBidiLSTM",
        "DepParUDLSTM",
        "Par2dLSTM",
        "Series",
        "Reconfig",
        "RTLReversed",
        "TTBReversed",
        "XYTranspose",
        "LSTM",
        "SummLSTM",
        "Logistic",
        "LinLogistic",
        "LinTanh",
        "Tanh",
        "Relu",
        "Linear",
        "Softmax",
        "SoftmaxNoCTC",
        "LSTMSoftmax",
        "LSTMBinarySoftmax",
        "TensorFlow",
    ];

    /// This type's `kTypeNames` string (the inverse of [`from_type_name`]).
    ///
    /// [`from_type_name`]: NetworkType::from_type_name
    #[inline]
    #[must_use]
    pub const fn type_name(self) -> &'static str {
        Self::TYPE_NAMES[self as usize]
    }

    /// Resolve an ordinal (`0..NT_COUNT`) to a [`NetworkType`] — the enum
    /// discriminant. `None` for `NT_COUNT` or beyond.
    #[inline]
    #[must_use]
    pub const fn from_ordinal(o: u8) -> Option<NetworkType> {
        // Exhaustive match: the compiler proves every real ordinal is covered.
        Some(match o {
            0 => NetworkType::None,
            1 => NetworkType::Input,
            2 => NetworkType::Convolve,
            3 => NetworkType::Maxpool,
            4 => NetworkType::Parallel,
            5 => NetworkType::Replicated,
            6 => NetworkType::ParRlLstm,
            7 => NetworkType::ParUdLstm,
            8 => NetworkType::Par2dLstm,
            9 => NetworkType::Series,
            10 => NetworkType::Reconfig,
            11 => NetworkType::XReversed,
            12 => NetworkType::YReversed,
            13 => NetworkType::XyTranspose,
            14 => NetworkType::Lstm,
            15 => NetworkType::LstmSummary,
            16 => NetworkType::Logistic,
            17 => NetworkType::PosClip,
            18 => NetworkType::SymClip,
            19 => NetworkType::Tanh,
            20 => NetworkType::Relu,
            21 => NetworkType::Linear,
            22 => NetworkType::Softmax,
            23 => NetworkType::SoftmaxNoCtc,
            24 => NetworkType::LstmSoftmax,
            25 => NetworkType::LstmSoftmaxEncoded,
            26 => NetworkType::TensorFlow,
            _ => return None,
        })
    }

    /// Resolve an on-wire type name to a [`NetworkType`] — the exact
    /// `getNetworkType` match loop (`network.cpp:201`): linear scan of
    /// [`TYPE_NAMES`]. `None` is `getNetworkType`'s `data == NT_COUNT` "Invalid
    /// network layer type" path.
    ///
    /// [`TYPE_NAMES`]: NetworkType::TYPE_NAMES
    #[inline]
    #[must_use]
    pub fn from_type_name(name: &str) -> Option<NetworkType> {
        let mut i = 0;
        while i < NT_COUNT {
            if Self::TYPE_NAMES[i] == name {
                return Self::from_ordinal(i as u8);
            }
            i += 1;
        }
        None
    }

    /// This layer type's full `classid` in the OCR domain: canon =
    /// [`NETWORK_LAYER`], custom = the type ordinal. The node's `facet_classid`;
    /// the `invoke_network` dispatch (the `invoke_unicharset` keystone analog)
    /// resolves the subclass by [`classid_custom`](crate::ogar_codebook::classid_custom).
    #[inline]
    #[must_use]
    pub fn classid(self) -> u32 {
        compose_classid(NETWORK_LAYER, self as u16)
    }
}

/// A parse error in a serialized [`NetworkHeader`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkError {
    /// The buffer ended before the base header was fully read.
    UnexpectedEof,
    /// The `tag` byte was not `NT_NONE`(0) — an unversioned/foreign blob
    /// (`getNetworkType` only branches into the string path when `tag == 0`).
    BadTag(i8),
    /// The `type_name` string did not match any [`NetworkType::TYPE_NAMES`]
    /// entry (`getNetworkType`'s `data == NT_COUNT` path).
    UnknownType,
    /// A negative dimension (`ni`/`no`/`num_weights` are non-negative for any
    /// serialized model).
    NegativeDim,
}

/// The base `Network` header shared by every layer node — the fields
/// `Network::CreateFromFile` reads before dispatching to the subclass
/// (`network.cpp:214-248`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkHeader {
    /// The layer subclass, from the `kTypeNames` on-wire discriminant.
    pub ntype: NetworkType,
    /// `TrainingState` byte (recognizer models serialize `TS_DISABLED`).
    pub training: i8,
    /// Whether the node needs to output back-deltas (`0`/`1`).
    pub needs_backprop: bool,
    /// `NetworkFlags` bits.
    pub network_flags: i32,
    /// Number of input values.
    pub ni: i32,
    /// Number of output values.
    pub no: i32,
    /// Number of weights in THIS node and its sub-network (cumulative).
    pub num_weights: i32,
    /// The layer's unique name.
    pub name: String,
}

impl NetworkHeader {
    /// Parse the base header from the front of `bytes`, returning the header and
    /// the number of bytes consumed (the offset at which the subclass payload
    /// begins). Rejects a non-zero tag, an unknown type name, and negative dims
    /// — a serialized model never carries them, so they signal a bad/foreign
    /// blob rather than silently mis-parsing (stricter than the C++ factory,
    /// which trusts its own output).
    pub fn from_le_bytes(bytes: &[u8]) -> Result<(NetworkHeader, usize), NetworkError> {
        let mut r = ByteReader::new(bytes);
        let tag = r.read_i8()?;
        if tag != 0 {
            return Err(NetworkError::BadTag(tag));
        }
        let type_name = r.read_string()?;
        let ntype = NetworkType::from_type_name(&type_name).ok_or(NetworkError::UnknownType)?;
        let training = r.read_i8()?;
        let needs_backprop = r.read_i8()? != 0;
        let network_flags = r.read_i32()?;
        let ni = r.read_i32()?;
        let no = r.read_i32()?;
        let num_weights = r.read_i32()?;
        if ni < 0 || no < 0 || num_weights < 0 {
            return Err(NetworkError::NegativeDim);
        }
        let name = r.read_string()?;
        Ok((
            NetworkHeader {
                ntype,
                training,
                needs_backprop,
                network_flags,
                ni,
                no,
                num_weights,
                name,
            },
            r.pos,
        ))
    }

    /// Sink this node onto the V3 SoA as a content-blind [`FacetCascade`] — the
    /// "16-byte tenant, classid + 12 bytes" substrate, read under
    /// [`CascadeShape::G6D2`](crate::facet::CascadeShape::G6D2) (six `u16` tiers).
    ///
    /// The `network_layer` ClassView projection of the 6 tiers:
    ///
    /// | tier | 8:8 `u16` | field |
    /// |---|---|---|
    /// | 0 | `ni` | inputs |
    /// | 1 | `no` | outputs |
    /// | 2 | `network_flags & 0xFFFF` | behaviour flags |
    /// | 3 | `num_weights` low 16 | cumulative weight count (lo) |
    /// | 4 | `num_weights` high 16 | cumulative weight count (hi) |
    /// | 5 | `training : needs_backprop` | lifecycle bytes (`lo:hi`) |
    ///
    /// `facet_classid` = [`NetworkType::classid`] (`NETWORK_LAYER : ntype`). The
    /// **name** is NOT bundled (`I-VSA-IDENTITIES`: the facet is the identity +
    /// typed dims; the name string is content in an out-of-line store keyed by the
    /// classid+identity). The **weights** are out-of-line too — only their `count`
    /// rides tiers 3-4; the blob is a separate Lance column. `ni`/`no`/flags are
    /// truncated to `u16` (every real eng.lstm dim is `< 65536`); a hypothetical
    /// `> u16` model would carry the overflow out-of-line, same as the weights.
    #[inline]
    #[must_use]
    pub fn to_facet(&self) -> FacetCascade {
        // ni/no are the semantic dims that MUST round-trip; every real eng.lstm dim
        // is < 65536, but a hypothetical wider model would truncate here silently.
        // Fail loudly in debug (mirrors the CANON mint-path `debug_assert`); a real
        // out-of-range dim is the trigger to add an out-of-line escape. `ni`/`no` are
        // non-negative (`NegativeDim` is rejected in `from_le_bytes`). `network_flags`
        // is a bitmask whose low-16 is the documented projection, not a dim, so it is
        // deliberately not asserted. The prefix-routing redouts (`hi_distance` etc.)
        // are NOT meaningful across the tiers-3/4 `num_weights` split — this facet is
        // read as 6× concatenated-`u16`, not as `hi`/`lo` prefix chains.
        debug_assert!(
            (self.ni as u32) <= u16::MAX as u32 && (self.no as u32) <= u16::MAX as u32,
            "network ni/no exceeds u16 — needs an out-of-line escape (network.rs::to_facet)"
        );
        let nw = self.num_weights as u32;
        FacetCascade {
            facet_classid: self.ntype.classid(),
            tiers: [
                tier_u16(self.ni as u32 as u16),
                tier_u16(self.no as u32 as u16),
                tier_u16(self.network_flags as u32 as u16),
                tier_u16((nw & 0xFFFF) as u16),
                tier_u16((nw >> 16) as u16),
                FacetTier {
                    lo: self.training as u8,
                    hi: u8::from(self.needs_backprop),
                },
            ],
        }
    }

    /// A one-line byte-parity dump (`type ni no num_weights name`) — the surface
    /// diffed against the C++ `network_spec_oracle`.
    #[must_use]
    pub fn dump(&self) -> String {
        format!(
            "{} ni={} no={} num_weights={} name={}",
            self.ntype.type_name(),
            self.ni,
            self.no,
            self.num_weights,
            self.name
        )
    }
}

/// One 8:8 [`FacetTier`] carrying a `u16` as `(hi, lo)` — the concatenated-`u16`
/// projection ([`FacetTier::as_u16`] is its inverse).
#[inline]
const fn tier_u16(v: u16) -> FacetTier {
    FacetTier {
        lo: (v & 0xFF) as u8,
        hi: (v >> 8) as u8,
    }
}

/// A forward-only little-endian cursor (the Core's per-module binary-read idiom;
/// mirrors [`crate::unicharcompress`]'s reader). `TFile::swap_ == false` on a LE
/// host, so scalars are raw `from_le_bytes`; a `std::string` is a `u32` length
/// prefix then that many raw bytes (`serialis.cpp:94-110`).
struct ByteReader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> ByteReader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], NetworkError> {
        let end = self.pos.checked_add(n).ok_or(NetworkError::UnexpectedEof)?;
        let slice = self
            .bytes
            .get(self.pos..end)
            .ok_or(NetworkError::UnexpectedEof)?;
        self.pos = end;
        Ok(slice)
    }

    fn read_i8(&mut self) -> Result<i8, NetworkError> {
        Ok(self.take(1)?[0] as i8)
    }

    fn read_i32(&mut self) -> Result<i32, NetworkError> {
        let arr: [u8; 4] = self
            .take(4)?
            .try_into()
            .map_err(|_| NetworkError::UnexpectedEof)?;
        Ok(i32::from_le_bytes(arr))
    }

    fn read_u32(&mut self) -> Result<u32, NetworkError> {
        let arr: [u8; 4] = self
            .take(4)?
            .try_into()
            .map_err(|_| NetworkError::UnexpectedEof)?;
        Ok(u32::from_le_bytes(arr))
    }

    /// A `TFile` `std::string`: `u32 len` then `len` raw bytes (`serialis.cpp:94-110`).
    fn read_string(&mut self) -> Result<String, NetworkError> {
        let len = self.read_u32()? as usize;
        let bytes = self.take(len)?;
        Ok(String::from_utf8_lossy(bytes).into_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facet::CascadeShape;
    use crate::ogar_codebook::{canonical_concept_id, classid_canon, classid_custom};

    #[test]
    fn network_layer_const_matches_codebook() {
        // The compile-lock: NETWORK_LAYER (used to build every facet_classid) must
        // equal the codebook's `network_layer` mint — else a rename/renumber on one
        // side silently mis-routes every network node's classid (core-first-architect
        // hygiene finding). The codebook is the single source of truth.
        assert_eq!(
            canonical_concept_id("network_layer"),
            Some(NETWORK_LAYER),
            "network_layer const drifted from the ogar_codebook mint"
        );
    }

    /// Build the base header a `Network::Serialize` would write for a node.
    fn header_bytes(type_name: &str, ni: i32, no: i32, num_weights: i32, name: &str) -> Vec<u8> {
        let mut b = Vec::new();
        b.push(0u8); // tag = NT_NONE
        b.extend_from_slice(&(type_name.len() as u32).to_le_bytes());
        b.extend_from_slice(type_name.as_bytes());
        b.push(0u8); // training = TS_DISABLED
        b.push(0u8); // needs_backprop = false
        b.extend_from_slice(&192i32.to_le_bytes()); // network_flags
        b.extend_from_slice(&ni.to_le_bytes());
        b.extend_from_slice(&no.to_le_bytes());
        b.extend_from_slice(&num_weights.to_le_bytes());
        b.extend_from_slice(&(name.len() as u32).to_le_bytes());
        b.extend_from_slice(name.as_bytes());
        b
    }

    #[test]
    fn type_names_round_trip_and_are_ordinal_aligned() {
        assert_eq!(NetworkType::TYPE_NAMES.len(), NT_COUNT);
        for o in 0..NT_COUNT as u8 {
            let nt = NetworkType::from_ordinal(o).expect("real ordinal");
            assert_eq!(nt as u8, o, "discriminant == ordinal");
            assert_eq!(nt.type_name(), NetworkType::TYPE_NAMES[o as usize]);
            assert_eq!(NetworkType::from_type_name(nt.type_name()), Some(nt));
        }
        assert_eq!(NetworkType::from_ordinal(NT_COUNT as u8), None);
        assert_eq!(NetworkType::from_type_name("NotAType"), None);
        // The wire discriminant is decoupled from the enum name (kTypeNames).
        assert_eq!(
            NetworkType::from_type_name("SummLSTM"),
            Some(NetworkType::LstmSummary)
        );
        assert_eq!(NetworkType::None.type_name(), "Invalid");
    }

    #[test]
    fn parses_pre_registered_eng_lstm_outer_header() {
        // The first-principles pre-registration: eng.lstm's outermost node
        // (module docs) — Series, ni=36, no=111, num_weights=385807. Built here
        // as the exact bytes Network::Serialize writes; the real-file parity is
        // the network_dump example vs the libtesseract oracle.
        let bytes = header_bytes("Series", 36, 111, 385807, "root");
        let (h, consumed) = NetworkHeader::from_le_bytes(&bytes).expect("valid header");
        assert_eq!(h.ntype, NetworkType::Series);
        assert_eq!(h.ni, 36);
        assert_eq!(h.no, 111);
        assert_eq!(h.num_weights, 385807);
        assert_eq!(h.name, "root");
        assert_eq!(
            consumed,
            bytes.len(),
            "base header consumes the whole prefix"
        );
        assert_eq!(h.dump(), "Series ni=36 no=111 num_weights=385807 name=root");
    }

    #[test]
    fn header_sinks_onto_g6d2_facet_losslessly() {
        let (h, _) = NetworkHeader::from_le_bytes(&header_bytes("LSTM", 48, 96, 55296, "L1"))
            .expect("valid");
        let f = h.to_facet();

        // facet_classid = network_layer(0x0804) canon : LSTM(14) custom.
        assert_eq!(classid_canon(f.facet_classid), NETWORK_LAYER);
        assert_eq!(classid_custom(f.facet_classid), NetworkType::Lstm as u16);
        assert_eq!(f.facet_classid, NetworkType::Lstm.classid());

        // Read the tiers back under the operator's 6x8:8 (G6D2) shape.
        let s = CascadeShape::G6D2;
        assert_eq!(s.levels(), 2, "6x8:8 = 6 groups x 2 levels");
        assert_eq!(f.tiers[0].as_u16(), 48, "tier0 = ni");
        assert_eq!(f.tiers[1].as_u16(), 96, "tier1 = no");
        assert_eq!(f.tiers[2].as_u16(), 192, "tier2 = network_flags low16");
        // num_weights 55296 = 0x0000_D800 → lo=0xD800(55296), hi=0.
        let nw = (f.tiers[3].as_u16() as u32) | ((f.tiers[4].as_u16() as u32) << 16);
        assert_eq!(nw, 55296, "tiers 3-4 = num_weights u32");
        assert_eq!(f.tiers[5].lo, 0, "training byte");
        assert_eq!(f.tiers[5].hi, 0, "needs_backprop byte");

        // The facet is exactly 16 bytes: classid(4) + 6x(8:8)=12.
        assert_eq!(f.to_bytes().len(), 16);
    }

    #[test]
    fn num_weights_high_half_survives_the_two_tiers() {
        // A cumulative count above u16 (the eng.lstm root is 385807) round-trips
        // through tiers 3-4 — the reason num_weights takes two 8:8 tiers.
        let (h, _) = NetworkHeader::from_le_bytes(&header_bytes("Series", 36, 111, 385807, "r"))
            .expect("ok");
        let f = h.to_facet();
        let nw = (f.tiers[3].as_u16() as u32) | ((f.tiers[4].as_u16() as u32) << 16);
        assert_eq!(nw, 385807);
        assert!(f.tiers[4].as_u16() > 0, "high half is non-zero for 385807");
    }

    #[test]
    fn rejects_bad_tag_and_short_and_unknown() {
        // Non-zero tag → BadTag.
        let mut b = header_bytes("Series", 1, 1, 0, "x");
        b[0] = 7;
        assert_eq!(
            NetworkHeader::from_le_bytes(&b),
            Err(NetworkError::BadTag(7))
        );

        // Truncated mid-header → UnexpectedEof.
        let full = header_bytes("Series", 1, 1, 0, "x");
        assert_eq!(
            NetworkHeader::from_le_bytes(&full[..10]),
            Err(NetworkError::UnexpectedEof)
        );

        // Unknown type string → UnknownType.
        let b = header_bytes("Frobnicate", 1, 1, 0, "x");
        assert_eq!(
            NetworkHeader::from_le_bytes(&b),
            Err(NetworkError::UnknownType)
        );

        // Negative dim → NegativeDim.
        let b = header_bytes("Series", -1, 1, 0, "x");
        assert_eq!(
            NetworkHeader::from_le_bytes(&b),
            Err(NetworkError::NegativeDim)
        );
    }
}
