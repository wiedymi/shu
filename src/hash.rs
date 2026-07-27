//! Small hashing helpers shared by cache and release-verification code.

use sha2::{Digest, Sha256};

/// Return the lowercase hexadecimal SHA-256 digest of `bytes`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::sha256_hex;

    #[test]
    fn encodes_a_sha256_digest_as_lowercase_hex() {
        assert_eq!(
            sha256_hex(b"shu"),
            "bef9a0ab123c0575c9ed42922b85625063b2704b22f965a3787fa38fc6511635"
        );
    }
}
