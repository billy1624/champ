use sha3::{Digest, Sha3_256};

pub fn sha3(data: impl AsRef<[u8]>) -> Vec<u8> {
    let mut hasher = Sha3_256::new();
    hasher.update(data);
    hasher.finalize()[..].to_vec()
}
