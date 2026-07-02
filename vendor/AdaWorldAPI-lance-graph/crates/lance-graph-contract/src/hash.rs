/// Canonical FNV-1a 64-bit hash.
///
/// Stable across Rust versions, platforms, and process restarts.
/// All crates in the workspace import this single implementation
/// instead of maintaining local copies.
///
/// `const fn` so it can be used in const contexts (e.g. role-key
/// seed computation in `grammar::role_keys`).
pub const fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    let mut i = 0;
    while i < bytes.len() {
        hash ^= bytes[i] as u64;
        hash = hash.wrapping_mul(0x100000001b3);
        i += 1;
    }
    hash
}

/// Convenience wrapper for string slices.
#[inline]
pub fn fnv1a_str(s: &str) -> u64 {
    fnv1a(s.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_vectors() {
        assert_eq!(fnv1a(b""), 0xcbf29ce484222325);
        assert_eq!(fnv1a(b"a"), 0xaf63dc4c8601ec8c);
        assert_eq!(fnv1a(b"foobar"), 0x85944171f73967e8);
    }

    #[test]
    fn str_matches_bytes() {
        assert_eq!(fnv1a_str("hello"), fnv1a(b"hello"));
    }

    #[test]
    fn different_inputs_differ() {
        assert_ne!(fnv1a(b"SUBJECT"), fnv1a(b"OBJECT"));
    }

    #[test]
    fn const_context_works() {
        const H: u64 = fnv1a(b"compile-time");
        assert_ne!(H, 0);
    }
}
