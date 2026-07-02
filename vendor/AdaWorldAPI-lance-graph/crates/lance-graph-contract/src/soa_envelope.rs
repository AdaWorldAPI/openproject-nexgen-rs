//! SoA envelope little-endian contract.
//!
//! # Why this module exists
//!
//! Column-level LE knowledge is not enough. ndarray's `MultiLaneColumn`
//! (the column carrier) already decodes its own bytes little-endian, and
//! `CausalEdge64` / `EpisodicEdges64` each know their own `to_le_bytes` /
//! `from_le_bytes`. But the **SoA envelope as a whole** — the thing a Lance
//! version snapshots, the thing `simd_soa` sweeps, the thing a future reader
//! decodes — has no contract describing how those columns *assemble* into one
//! row-strided packet. The parts know the LE contract; the envelope did not.
//!
//! [`SoaEnvelope`] is that missing contract. It makes the in-place SoA
//! backing store **self-describing at each cycle**: a stable column ordering,
//! a fixed row byte stride, a `cycle` version stamp, and a
//! [`ENVELOPE_LAYOUT_VERSION`]. With it, a Lance version IS a coherent LE
//! in-place layout at cycle N — not a loose collection of independently-
//! correct columns. Nothing is serialized or transmitted; the backing bytes
//! are resident in-place, zero-copy from creation to Lance tombstone.
//!
//! # Layering (read before adding an ndarray dependency here)
//!
//! This module is **zero-dep, byte-geometry only**. It describes *where*
//! columns sit in the backing store's row stride and *what* LE element each
//! holds — as data ([`ColumnDescriptor`]), never as ndarray generic bounds.
//! That keeps `lance-graph-contract` featherweight for its non-HPC consumers
//! (OGAR classes, ractor actors), and it keeps ndarray usable standalone by
//! any pure-SIMD consumer.
//!
//! The split is deliberate and complementary, not duplicated:
//!
//! | Level | Home | Answers |
//! |-------|------|---------|
//! | Column LE contract | `ndarray::simd::MultiLaneColumn` | "how do I sweep one typed column" |
//! | Envelope LE contract | this module | "where do columns sit in the row stride, what cycle is this" |
//! | Composition | `lance-graph` (always has both deps) | carve envelope columns → wrap each in `MultiLaneColumn` |
//!
//! ndarray never learns the envelope exists; this crate never learns ndarray
//! exists; `lance-graph` binds them.

/// Layout version of the envelope byte geometry.
///
/// Bumped whenever the meaning of [`ColumnDescriptor`] offsets/strides
/// changes. A reader MUST refuse to decode a packet whose stamped version it
/// does not understand (per `I-LEGACY-API-FEATURE-GATED`: layout reclaim is
/// paired with a version gate on the serialization path).
///
/// - **v1** — initial canonical `NodeRow` value carve.
/// - **v2** — `HelixResidue` value-tenant right-sized 48 B → 6 B (a bits→bytes
///   slip fix), which shifted every downstream tenant offset (`TurbovecResidue`
///   160→118, `Energy` 176→134, …). The offsets moved, so the version gates it:
///   a v1 blob now refuses to decode rather than read tenants from the wrong
///   bytes. Safe because nothing persisted under v1 (FULL is POC-only).
pub const ENVELOPE_LAYOUT_VERSION: u8 = 2;

/// The little-endian element type of one column.
///
/// Width only — no distance semantics, no domain meaning (cf. ndarray's
/// no-umbrella rule). The actual decode (`from_le_bytes`) happens in the
/// consumer's `MultiLaneColumn` lane iterator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum ColumnKind {
    U8 = 0,
    I8 = 1,
    U16 = 2,
    I16 = 3,
    U32 = 4,
    F32 = 5,
    U64 = 6,
    F64 = 7,
}

impl ColumnKind {
    /// Bytes per element of this LE column kind.
    pub const fn elem_bytes(self) -> usize {
        match self {
            ColumnKind::U8 | ColumnKind::I8 => 1,
            ColumnKind::U16 | ColumnKind::I16 => 2,
            ColumnKind::U32 | ColumnKind::F32 => 4,
            ColumnKind::U64 | ColumnKind::F64 => 8,
        }
    }
}

/// One column's placement within a single row of the backing store.
///
/// `Copy` and `repr(C)` so a descriptor table is itself a stable LE artifact.
/// `name_id` is a stable column ordinal (an enum discriminant on the consumer
/// side), NOT a string — keeping this crate alloc-free and the descriptor
/// `Copy`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(C)]
pub struct ColumnDescriptor {
    /// Stable column identity (consumer-side enum ordinal).
    pub name_id: u16,
    /// LE element kind.
    pub kind: ColumnKind,
    /// Elements of `kind` per row for this column (e.g. content = 256 × u64,
    /// energy = 1 × f32).
    pub elems_per_row: u16,
    /// Byte offset of this column within one row packet.
    pub row_offset: u32,
}

impl ColumnDescriptor {
    /// Bytes this column occupies in one row.
    pub const fn col_bytes_per_row(&self) -> usize {
        self.kind.elem_bytes() * self.elems_per_row as usize
    }

    /// Byte range `[start, end)` of this column within a row packet.
    pub const fn row_byte_range(&self) -> (usize, usize) {
        let start = self.row_offset as usize;
        (start, start + self.col_bytes_per_row())
    }
}

/// What can go wrong validating an envelope's byte geometry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnvelopeError {
    /// The stamped layout version is not the one this build understands.
    LayoutVersionMismatch { expected: u8, found: u8 },
    /// Sum of column byte-widths does not equal the declared row stride.
    StrideMismatch { declared: usize, summed: usize },
    /// Two columns overlap, or a gap/ordering violation was found.
    ColumnOverlap { col_a: u16, col_b: u16 },
    /// A column's byte range ends past the declared row stride. Distinct from
    /// [`StrideMismatch`]: the widths can sum to the stride while a column is
    /// still positioned (via its `row_offset`) so its end exceeds the stride.
    ColumnOutOfBounds {
        col: u16,
        col_end: usize,
        stride: usize,
    },
    /// `as_le_bytes().len()` is not `row_stride * n_rows` (backing store size mismatch).
    PacketSizeMismatch { expected: usize, found: usize },
    /// A requested row or column index is out of bounds.
    OutOfBounds,
}

/// The little-endian geometry contract for one SoA envelope cycle.
///
/// Implemented by the owner of the in-place backing store (e.g. the mailbox
/// SoA). The envelope is zero-copy from creation to Lance tombstone — nothing
/// is serialized or transmitted; this trait describes *where columns sit* in
/// the already-resident backing bytes and *what cycle stamp* the store carries.
/// The read-only view here mirrors `MailboxSoaView` vs `MailboxSoaOwner`:
/// mutation lives on the owner type, never on this trait.
pub trait SoaEnvelope {
    /// Layout version this implementor's geometry conforms to.
    const LAYOUT_VERSION: u8 = ENVELOPE_LAYOUT_VERSION;

    /// Stable, ordered column placement table. Ordering is part of the
    /// contract: a reader walks columns in this order.
    fn columns(&self) -> &[ColumnDescriptor];

    /// Total bytes per row across all columns.
    fn row_stride(&self) -> usize;

    /// Number of rows in this snapshot.
    fn n_rows(&self) -> usize;

    /// The version stamp this snapshot carries (the cycle whose committed
    /// state these bytes are). This is what turns a Lance version into a
    /// coherent "packet at cycle N".
    fn cycle(&self) -> u32;

    /// The whole packet as contiguous LE bytes, zero-copy. Length MUST be
    /// `row_stride() * n_rows()`.
    fn as_le_bytes(&self) -> &[u8];

    /// Zero-copy LE view of one full row.
    fn row_le(&self, row: usize) -> Option<&[u8]> {
        let stride = self.row_stride();
        let start = row.checked_mul(stride)?;
        let end = start.checked_add(stride)?;
        self.as_le_bytes().get(start..end)
    }

    /// Zero-copy LE view of one column within one row.
    fn column_le(&self, row: usize, col: &ColumnDescriptor) -> Option<&[u8]> {
        let r = self.row_le(row)?;
        let (start, end) = col.row_byte_range();
        r.get(start..end)
    }

    /// Validate that the declared geometry is internally consistent and that
    /// the backing packet matches. Call this at the Lance read boundary — a
    /// v1 packet under a v2 reader (or a torn snapshot) is refused here rather
    /// than silently mis-decoded downstream.
    fn verify_layout(&self) -> Result<(), EnvelopeError> {
        // 1. Version gate.
        if Self::LAYOUT_VERSION != ENVELOPE_LAYOUT_VERSION {
            return Err(EnvelopeError::LayoutVersionMismatch {
                expected: ENVELOPE_LAYOUT_VERSION,
                found: Self::LAYOUT_VERSION,
            });
        }
        // 2. Columns are non-overlapping, each fits within [0, stride), and
        //    their widths sum to the stride.
        //    Checking only the width sum is insufficient: two columns whose
        //    widths sum to the stride can still have one column whose end
        //    offset exceeds the stride (e.g. offsets 4+8 with stride 8).
        let cols = self.columns();
        let mut summed = 0usize;
        let stride = self.row_stride();
        // `row_offset as usize + col_bytes` can wrap on 32-bit targets (wasm32):
        // row_offset is u32 (≤ 4.29e9) and col_bytes can reach 8 × 65535, so the
        // sum can exceed usize::MAX on a 32-bit usize and wrap to a small value
        // that would slip past the `a_end > stride` check. Compute every end with
        // checked_add and reject overflow as ColumnOutOfBounds.
        let checked_end = |c: &ColumnDescriptor| -> Result<usize, EnvelopeError> {
            (c.row_offset as usize)
                .checked_add(c.col_bytes_per_row())
                .ok_or(EnvelopeError::ColumnOutOfBounds {
                    col: c.name_id,
                    col_end: usize::MAX,
                    stride,
                })
        };
        for (i, a) in cols.iter().enumerate() {
            let a_start = a.row_offset as usize;
            let a_end = checked_end(a)?;
            summed += a.col_bytes_per_row();
            if a_end > stride {
                return Err(EnvelopeError::ColumnOutOfBounds {
                    col: a.name_id,
                    col_end: a_end,
                    stride,
                });
            }
            for b in &cols[i + 1..] {
                let b_start = b.row_offset as usize;
                let b_end = checked_end(b)?;
                let overlap = a_start < b_end && b_start < a_end;
                if overlap {
                    return Err(EnvelopeError::ColumnOverlap {
                        col_a: a.name_id,
                        col_b: b.name_id,
                    });
                }
            }
        }
        if summed != stride {
            return Err(EnvelopeError::StrideMismatch {
                declared: stride,
                summed,
            });
        }
        // 3. Backing packet size matches stride × rows.
        let expected = stride.saturating_mul(self.n_rows());
        let found = self.as_le_bytes().len();
        if expected != found {
            return Err(EnvelopeError::PacketSizeMismatch { expected, found });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestEnvelope {
        cols: Vec<ColumnDescriptor>,
        stride: usize,
        rows: usize,
        bytes: Vec<u8>,
        cycle: u32,
    }

    impl SoaEnvelope for TestEnvelope {
        fn columns(&self) -> &[ColumnDescriptor] {
            &self.cols
        }
        fn row_stride(&self) -> usize {
            self.stride
        }
        fn n_rows(&self) -> usize {
            self.rows
        }
        fn cycle(&self) -> u32 {
            self.cycle
        }
        fn as_le_bytes(&self) -> &[u8] {
            &self.bytes
        }
    }

    fn two_col_envelope(rows: usize) -> TestEnvelope {
        // col 0: 1 × f32 (4 B) at offset 0
        // col 1: 1 × u64 (8 B) at offset 4
        let cols = vec![
            ColumnDescriptor {
                name_id: 0,
                kind: ColumnKind::F32,
                elems_per_row: 1,
                row_offset: 0,
            },
            ColumnDescriptor {
                name_id: 1,
                kind: ColumnKind::U64,
                elems_per_row: 1,
                row_offset: 4,
            },
        ];
        let stride = 12;
        TestEnvelope {
            cols,
            stride,
            rows,
            bytes: vec![0u8; stride * rows],
            cycle: 7,
        }
    }

    #[test]
    fn kind_widths() {
        assert_eq!(ColumnKind::U8.elem_bytes(), 1);
        assert_eq!(ColumnKind::F32.elem_bytes(), 4);
        assert_eq!(ColumnKind::U64.elem_bytes(), 8);
    }

    #[test]
    fn descriptor_byte_range() {
        let d = ColumnDescriptor {
            name_id: 0,
            kind: ColumnKind::U64,
            elems_per_row: 256,
            row_offset: 16,
        };
        assert_eq!(d.col_bytes_per_row(), 256 * 8);
        assert_eq!(d.row_byte_range(), (16, 16 + 256 * 8));
    }

    #[test]
    fn valid_envelope_passes() {
        let env = two_col_envelope(4);
        assert_eq!(env.cycle(), 7);
        assert!(env.verify_layout().is_ok());
    }

    #[test]
    fn stride_mismatch_caught() {
        let mut env = two_col_envelope(4);
        env.stride = 16; // columns sum to 12, not 16
        env.bytes = vec![0u8; 16 * 4];
        assert_eq!(
            env.verify_layout(),
            Err(EnvelopeError::StrideMismatch {
                declared: 16,
                summed: 12,
            })
        );
    }

    #[test]
    fn overlap_caught() {
        let mut env = two_col_envelope(1);
        env.cols[1].row_offset = 2; // u64 at 2 overlaps f32 at [0,4)
        env.stride = 10;
        env.bytes = vec![0u8; 10];
        assert!(matches!(
            env.verify_layout(),
            Err(EnvelopeError::ColumnOverlap { .. })
        ));
    }

    #[test]
    fn column_past_stride_caught() {
        // Two 4-byte columns at offsets 4 and 8 with stride 8.
        // Width sum = 8 = stride, but column B's end (12) > stride (8).
        let cols = vec![
            ColumnDescriptor {
                name_id: 0,
                kind: ColumnKind::F32,
                elems_per_row: 1,
                row_offset: 4,
            },
            ColumnDescriptor {
                name_id: 1,
                kind: ColumnKind::F32,
                elems_per_row: 1,
                row_offset: 8,
            },
        ];
        let env = TestEnvelope {
            cols,
            stride: 8,
            rows: 1,
            bytes: vec![0u8; 8],
            cycle: 0,
        };
        assert!(matches!(
            env.verify_layout(),
            Err(EnvelopeError::ColumnOutOfBounds {
                col: 1,
                col_end: 12,
                stride: 8
            })
        ));
    }

    #[test]
    fn packet_size_mismatch_caught() {
        let mut env = two_col_envelope(4);
        env.bytes.truncate(12 * 3); // one row short
        assert_eq!(
            env.verify_layout(),
            Err(EnvelopeError::PacketSizeMismatch {
                expected: 48,
                found: 36,
            })
        );
    }

    #[test]
    fn row_and_column_views_are_zero_copy_slices() {
        let mut env = two_col_envelope(2);
        // Write row 1, col 1 (u64) = 0x0102030405060708 LE.
        let v: u64 = 0x0102_0304_0506_0708;
        let row1_col1_start = 12 + 4;
        env.bytes[row1_col1_start..row1_col1_start + 8].copy_from_slice(&v.to_le_bytes());

        let row = env.row_le(1).unwrap();
        assert_eq!(row.len(), 12);

        let col = env.column_le(1, &env.cols[1]).unwrap();
        assert_eq!(col.len(), 8);
        assert_eq!(u64::from_le_bytes(col.try_into().unwrap()), v);

        // Out of bounds.
        assert!(env.row_le(2).is_none());
    }
}
