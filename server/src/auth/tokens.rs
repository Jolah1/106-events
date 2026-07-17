use data_encoding::BASE64URL_NOPAD;
use sha2::{Digest, Sha256};

/// A freshly generated secret token: the raw value goes to the client (URL or
/// cookie), only the hash is ever stored.
pub struct NewToken {
    pub raw: String,
    pub hash: String,
}

pub fn generate_token() -> NewToken {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("os rng unavailable");
    let raw = BASE64URL_NOPAD.encode(&bytes);
    let hash = hash_token(&raw);
    NewToken { raw, hash }
}

pub fn hash_token(raw: &str) -> String {
    let digest = Sha256::digest(raw.as_bytes());
    BASE64URL_NOPAD.encode(&digest)
}
