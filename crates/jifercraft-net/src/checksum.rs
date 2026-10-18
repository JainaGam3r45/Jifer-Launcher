use std::fmt;
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

use sha1::Digest;

use crate::error::NetError;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Checksum {
    /// Mojang metadata, libraries and assets.
    Sha1([u8; 20]),
    /// Adoptium runtime archives.
    Sha256([u8; 32]),
}

impl Checksum {
    pub fn sha1_hex(hex_digest: &str) -> Result<Self, NetError> {
        decode(hex_digest).map(Self::Sha1)
    }

    pub fn sha256_hex(hex_digest: &str) -> Result<Self, NetError> {
        decode(hex_digest).map(Self::Sha256)
    }

    pub fn hasher(&self) -> Hasher {
        match self {
            Self::Sha1(_) => Hasher::Sha1(sha1::Sha1::new()),
            Self::Sha256(_) => Hasher::Sha256(sha2::Sha256::new()),
        }
    }

    pub fn algorithm(&self) -> &'static str {
        match self {
            Self::Sha1(_) => "sha1",
            Self::Sha256(_) => "sha256",
        }
    }

    pub fn to_hex(&self) -> String {
        match self {
            Self::Sha1(bytes) => hex::encode(bytes),
            Self::Sha256(bytes) => hex::encode(bytes),
        }
    }

    /// Hashes an existing file with the same algorithm as `self`. Blocking.
    pub fn matches_file(&self, path: &Path) -> io::Result<bool> {
        let mut file = File::open(path)?;
        let mut hasher = self.hasher();
        let mut buffer = vec![0_u8; 64 * 1024];
        loop {
            let read = file.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
        }
        Ok(hasher.finish() == *self)
    }
}

impl fmt::Debug for Checksum {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.algorithm(), self.to_hex())
    }
}

impl fmt::Display for Checksum {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

#[derive(Debug, Clone)]
pub enum Hasher {
    Sha1(sha1::Sha1),
    Sha256(sha2::Sha256),
}

impl Hasher {
    pub fn update(&mut self, chunk: &[u8]) {
        match self {
            Self::Sha1(hasher) => hasher.update(chunk),
            Self::Sha256(hasher) => hasher.update(chunk),
        }
    }

    pub fn finish(self) -> Checksum {
        match self {
            Self::Sha1(hasher) => Checksum::Sha1(hasher.finalize().into()),
            Self::Sha256(hasher) => Checksum::Sha256(hasher.finalize().into()),
        }
    }
}

fn decode<const N: usize>(hex_digest: &str) -> Result<[u8; N], NetError> {
    let mut out = [0_u8; N];
    hex::decode_to_slice(hex_digest.trim(), &mut out).map_err(|_| NetError::InvalidChecksum(hex_digest.to_owned()))?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const HELLO_SHA1: &str = "aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d";
    const HELLO_SHA256: &str = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";

    #[test]
    fn hashes_known_vectors() {
        for expected in [
            Checksum::sha1_hex(HELLO_SHA1).unwrap(),
            Checksum::sha256_hex(HELLO_SHA256).unwrap(),
        ] {
            let mut hasher = expected.hasher();
            hasher.update(b"hel");
            hasher.update(b"lo");
            assert_eq!(hasher.finish(), expected);
        }
    }

    #[test]
    fn rejects_malformed_literals() {
        assert!(Checksum::sha1_hex("xyz").is_err());
        assert!(Checksum::sha1_hex(HELLO_SHA256).is_err(), "wrong length");
        assert!(Checksum::sha256_hex(HELLO_SHA1).is_err());
    }

    #[test]
    fn verifies_files_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hello.txt");
        std::fs::write(&path, "hello").unwrap();
        assert!(Checksum::sha1_hex(HELLO_SHA1).unwrap().matches_file(&path).unwrap());
        std::fs::write(&path, "hellO").unwrap();
        assert!(!Checksum::sha1_hex(HELLO_SHA1).unwrap().matches_file(&path).unwrap());
    }
}
