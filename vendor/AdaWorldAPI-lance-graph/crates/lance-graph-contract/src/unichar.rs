//! `UNICHAR` UTF-8 codec — the second byte-parity adapter of the Tesseract
//! C++→Rust transcode (the sibling of `unicharset`, this time on
//! `ccutil/unichar.cpp`).
//!
//! Tesseract's `UNICHAR` is the hand-rolled UTF-8 layer that `UNICHARSET` sits
//! on top of. Two pure functions carry the whole bytes↔codepoint substrate:
//! [`utf8_step`] (how many bytes the first character occupies, read from a
//! 256-entry lead-byte table) and [`utf8_to_utf32`] (decode a byte string to
//! its Unicode codepoints). Both are pure text — zero leptonica, zero `Pix` —
//! so this is a second zero-C-dependency byte-parity surface, exactly like
//! [`crate::unicharset`].
//!
//! # Why a faithful transcode, not [`core::str`]
//!
//! Tesseract's `utf8_step` table maps `0xC0`/`0xC1` (the overlong 2-byte leads)
//! to **step 2**, and [`utf8_to_utf32`] decodes the overlong NUL `C0 80` to
//! codepoint `0` — Rust's native [`core::str::from_utf8`] *rejects* both as
//! invalid UTF-8. A "just call `from_utf8`" shortcut would therefore DIVERGE
//! from the C++ oracle on real inputs. Reproducing the exact lead-byte table,
//! quirks and all, is the core-first doctrine and the behaviour-preservation
//! rule made concrete: a transcode mirrors the algorithm, it does not "fix" it.
//! The [`tests::from_utf8_rejects_what_tesseract_accepts`] test pins this gap.
//!
//! # Byte-parity surface (`PROBE-OGAR-ADAPTER-UNICHARSET` sibling)
//!
//! The `unichar_dump` example renders the exact TSV shape a small libtesseract
//! oracle prints — all 256 `utf8_step` values plus `utf8_to_utf32` over a
//! curated hex corpus — so byte-parity is a single `diff`. The exhaustive
//! 256-byte step table makes that half of the proof *complete*, not sampled.
//!
//! Mirrors `ccutil/unichar.cpp`: `utf8_step` (the 256-table, lines 143-156),
//! `first_uni` (the offset decode, lines 105-131), and `UTF8ToUTF32`
//! (lines 220-234 — lead-byte validation only; continuation bytes are NOT
//! re-checked, which is what lets `C0 80` through).

/// The number of bytes the first UTF-8 character of a buffer occupies, keyed on
/// its lead byte: `1`/`2`/`3`/`4` for a legal lead, `0` for an illegal one
/// (a continuation byte `0x80..=0xBF`, or `0xF8..=0xFF`).
///
/// This is a `const fn` transcription of Tesseract's 256-entry `utf8_bytes`
/// table (`unichar.cpp:143`). The exhaustive 256-byte byte-parity probe proves
/// it equals the C++ table value-for-value.
///
/// Note the faithfully-preserved quirk: `0xC0`/`0xC1` (the overlong 2-byte
/// leads) map to `2`, not `0` — Tesseract does not reject overlong forms here,
/// so neither does this transcode.
#[must_use]
pub const fn utf8_step(lead: u8) -> u8 {
    match lead {
        0x00..=0x7F => 1, // ASCII
        0x80..=0xBF => 0, // continuation byte — illegal as a lead
        0xC0..=0xDF => 2, // 2-byte lead (incl. overlong 0xC0/0xC1, faithfully)
        0xE0..=0xEF => 3, // 3-byte lead
        0xF0..=0xF7 => 4, // 4-byte lead
        0xF8..=0xFF => 0, // illegal
    }
}

/// UCS-4 offsets subtracted after accumulating the UTF-8 bytes, indexed by the
/// character's byte length (`unichar.cpp:106`). Indices 0/1 are `0`; the rest
/// cancel the accumulated lead-byte and continuation marker bits.
const UTF8_OFFSETS: [i32; 5] = [0, 0, 0x3080, 0xE2080, 0x3C8_2080];

/// The Unicode codepoint of the first character in `bytes`, mirroring
/// `UNICHAR::first_uni` (`unichar.cpp:105`). The only caller, [`utf8_to_utf32`],
/// guarantees `bytes` is non-empty, begins with a legal lead, AND contains all
/// `utf8_step(bytes[0])` bytes of that character (it rejects truncated trailing
/// sequences *before* calling). The `.take(len)` is therefore defensive — it can
/// never read past the slice even if that guarantee were ever violated.
fn first_uni(bytes: &[u8]) -> i32 {
    let len = utf8_step(bytes[0]) as usize;
    let mut uni: i32 = 0;
    for (i, &b) in bytes.iter().take(len).enumerate() {
        uni += i32::from(b);
        if i + 1 < len {
            uni <<= 6;
        }
    }
    uni - UTF8_OFFSETS[len.min(4)]
}

/// Decode a UTF-8 byte string to its Unicode codepoints, mirroring
/// `UNICHAR::UTF8ToUTF32` (`unichar.cpp:220`).
///
/// Returns `None` if any character's **lead byte** is illegal (the C++
/// "return an empty vector" path) OR if the input ends mid-character (a trailing
/// multibyte lead whose continuation bytes are not all present). The C++ reads
/// past its buffer in that truncation case (UB on a length-delimited slice);
/// this length-delimited decoder rejects it instead of fabricating a codepoint
/// from the partial bytes. Like the C++, continuation bytes that ARE present are
/// not re-validated, so the overlong NUL `C0 80` decodes to `[0]` rather than
/// being rejected (see the module docs). Empty input is `Some(vec![])` —
/// distinct from the `None` illegal case, which the C++ conflates as an empty
/// vector (the corpus avoids empty input so the byte-parity diff is unaffected).
#[must_use]
pub fn utf8_to_utf32(bytes: &[u8]) -> Option<Vec<i32>> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let step = utf8_step(bytes[i]) as usize;
        if step == 0 {
            return None; // illegal lead
        }
        if i + step > bytes.len() {
            return None; // truncated trailing multibyte sequence
        }
        out.push(first_uni(&bytes[i..]));
        i += step;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The `const fn` ranges reproduce the documented 256-table shape: 128 ones,
    /// 72 zeros (64 continuation + 8 `0xF8..`), 32 twos, 16 threes, 8 fours.
    #[test]
    fn step_table_value_histogram() {
        let mut counts = [0usize; 5];
        for b in 0u16..=255 {
            counts[utf8_step(b as u8) as usize] += 1;
        }
        assert_eq!(counts, [72, 128, 32, 16, 8]);
    }

    /// The faithfully-preserved quirks: overlong 2-byte leads are "valid"
    /// (step 2), continuation bytes and `0xF8..` are illegal (step 0).
    #[test]
    fn step_quirks_match_tesseract() {
        assert_eq!(utf8_step(0xC0), 2, "overlong lead kept as step 2");
        assert_eq!(utf8_step(0xC1), 2, "overlong lead kept as step 2");
        assert_eq!(utf8_step(0x80), 0, "continuation byte illegal as lead");
        assert_eq!(utf8_step(0xBF), 0, "continuation byte illegal as lead");
        assert_eq!(utf8_step(0xF8), 0, "5-byte form illegal");
        assert_eq!(utf8_step(0xFF), 0, "0xFF illegal");
        assert_eq!(utf8_step(b'A'), 1);
        assert_eq!(utf8_step(0xE4), 3);
        assert_eq!(utf8_step(0xF0), 4);
    }

    #[test]
    fn decodes_one_to_four_byte_chars() {
        assert_eq!(utf8_to_utf32(b"A"), Some(vec![65]));
        assert_eq!(utf8_to_utf32(&[0xC3, 0xA9]), Some(vec![0xE9])); // é U+00E9
        assert_eq!(utf8_to_utf32(&[0xE4, 0xB8, 0xAD]), Some(vec![0x4E2D])); // 中
        assert_eq!(
            utf8_to_utf32(&[0xF0, 0x9F, 0x98, 0x80]),
            Some(vec![0x1_F600]) // 😀 U+1F600 = 128512
        );
    }

    #[test]
    fn decodes_multi_char_strings() {
        assert_eq!(utf8_to_utf32(b"ABC"), Some(vec![65, 66, 67]));
        // "中文" = U+4E2D U+6587
        assert_eq!(
            utf8_to_utf32(&[0xE4, 0xB8, 0xAD, 0xE6, 0x96, 0x87]),
            Some(vec![0x4E2D, 0x6587])
        );
    }

    #[test]
    fn illegal_lead_byte_is_none() {
        assert_eq!(utf8_to_utf32(&[0x80]), None);
        assert_eq!(utf8_to_utf32(&[0xFF]), None);
        assert_eq!(utf8_to_utf32(&[b'A', 0xF8]), None); // legal then illegal
    }

    /// A truncated trailing multibyte sequence (legal lead, but its continuation
    /// bytes are not all present) is rejected — NOT decoded into a fabricated
    /// codepoint. The C++ reads past its buffer here (UB); this length-delimited
    /// decoder returns `None`. (Codex P2 on PR #534.)
    #[test]
    fn truncated_trailing_multibyte_is_rejected() {
        assert_eq!(utf8_to_utf32(&[0xC3]), None, "2-byte lead, 1 byte present");
        assert_eq!(utf8_to_utf32(&[0xE4, 0xB8]), None, "3-byte lead, 2 present");
        assert_eq!(
            utf8_to_utf32(&[0xF0, 0x9F, 0x98]),
            None,
            "4-byte lead, 3 present"
        );
        // Truncation after a valid char is rejected too (the whole decode fails).
        assert_eq!(utf8_to_utf32(&[b'a', 0xE4, 0xB8]), None);
        // A COMPLETE multibyte char is still accepted (regression guard).
        assert_eq!(utf8_to_utf32(&[0xC3, 0xA9]), Some(vec![0xE9]));
    }

    /// The overlong NUL `C0 80` is accepted and decodes to `0` — the defining
    /// quirk the byte-parity probe confirmed against the C++ oracle.
    #[test]
    fn overlong_nul_decodes_to_zero() {
        assert_eq!(utf8_to_utf32(&[0xC0, 0x80]), Some(vec![0]));
    }

    #[test]
    fn empty_input_is_some_empty() {
        assert_eq!(utf8_to_utf32(b""), Some(vec![]));
    }

    /// The whole reason this is a transcode and not a `from_utf8` call: Rust's
    /// native decoder rejects exactly the inputs Tesseract's hand-rolled table
    /// accepts. If these ever agreed, the faithful transcode would be moot.
    #[test]
    #[expect(
        invalid_from_utf8,
        reason = "the point of this test IS that the literal is invalid UTF-8 that std rejects but Tesseract accepts"
    )]
    fn from_utf8_rejects_what_tesseract_accepts() {
        // overlong NUL: std rejects, Tesseract decodes to [0]
        assert!(core::str::from_utf8(&[0xC0, 0x80]).is_err());
        assert_eq!(utf8_to_utf32(&[0xC0, 0x80]), Some(vec![0]));
        // 0xC0 lead: std rejects as invalid, Tesseract treats as a 2-byte lead
        assert_eq!(utf8_step(0xC0), 2);
    }
}
