//! Salted Argon2id password verification. Plaintext passwords are never serialized.
use argon2::{Algorithm, Argon2, Params, Version};
use rand::Rng as _;
use serde::{Deserialize, Serialize};
use subtle::ConstantTimeEq as _;
use zeroize::Zeroizing;

/// What Argon2id is asked to spend on one guess.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Cost {
    memory_kib: u32,
    iterations: u32,
    lanes: u32,
}

/// New verifiers: four times the memory of RFC 9106's second recommendation.
const COST: Cost = Cost {
    memory_kib: 256 * 1024,
    iterations: 3,
    lanes: 4,
};
/// Version 1 verifiers, which did not say their cost.
const FIRST_COST: Cost = Cost {
    memory_kib: 64 * 1024,
    iterations: 3,
    lanes: 1,
};
/// A record asking for more is not one ShellRS wrote.
const MOST: Cost = Cost {
    memory_kib: 4 * 1024 * 1024,
    iterations: 64,
    lanes: 64,
};
const VERSION: u8 = 2;
const SALT_LEN: usize = 32;
const HASH_LEN: usize = 32;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Verifier {
    version: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cost: Option<Cost>,
    salt: Vec<u8>,
    hash: Vec<u8>,
}

impl Verifier {
    pub(super) fn new(password: &str) -> anyhow::Result<Self> {
        let mut salt = vec![0; SALT_LEN];
        rand::rng().fill_bytes(&mut salt);
        let hash = derive(password, &salt, COST)?.to_vec();
        Ok(Self {
            version: VERSION,
            cost: Some(COST),
            salt,
            hash,
        })
    }

    fn checked_cost(&self) -> anyhow::Result<Cost> {
        anyhow::ensure!(self.hash.len() == HASH_LEN, "malformed app lock hash");
        match (self.version, self.cost) {
            (1, None) => {
                anyhow::ensure!(self.salt.len() == 16, "malformed app lock salt");
                Ok(FIRST_COST)
            }
            (VERSION, Some(cost)) => {
                anyhow::ensure!(self.salt.len() == SALT_LEN, "malformed app lock salt");
                anyhow::ensure!(
                    cost.memory_kib <= MOST.memory_kib
                        && cost.iterations <= MOST.iterations
                        && cost.lanes <= MOST.lanes,
                    "unsupported app lock cost"
                );
                Ok(cost)
            }
            _ => anyhow::bail!("unsupported app lock version"),
        }
    }

    pub(super) fn validate(&self) -> anyhow::Result<()> {
        self.checked_cost().map(drop)
    }

    pub(super) fn verify(&self, password: &str) -> anyhow::Result<bool> {
        let hash = derive(password, &self.salt, self.checked_cost()?)?;
        Ok(bool::from(self.hash.ct_eq(hash.as_ref())))
    }

    /// Whether it was made with less than new verifiers are.
    pub(super) fn is_outdated(&self) -> bool {
        self.version != VERSION || self.cost != Some(COST)
    }
}

fn derive(password: &str, salt: &[u8], cost: Cost) -> anyhow::Result<Zeroizing<[u8; HASH_LEN]>> {
    let params = Params::new(cost.memory_kib, cost.iterations, cost.lanes, Some(HASH_LEN))
        .map_err(|e| anyhow::anyhow!(e))?;
    let mut hash = Zeroizing::new([0; HASH_LEN]);
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(password.as_bytes(), salt, hash.as_mut())
        .map_err(|e| anyhow::anyhow!(e))?;
    Ok(hash)
}

#[cfg(test)]
pub(super) mod tests {
    use super::{COST, FIRST_COST, Verifier, derive};

    /// A verifier as version 1 wrote it.
    pub(crate) fn first_version(password: &str) -> Verifier {
        let salt = vec![7; 16];
        Verifier {
            version: 1,
            cost: None,
            hash: derive(password, &salt, FIRST_COST).unwrap().to_vec(),
            salt,
        }
    }

    #[test]
    fn passwords_are_salted_and_round_trip_without_plaintext() {
        let first = Verifier::new("a long 密码 123").unwrap();
        let second = Verifier::new("a long 密码 123").unwrap();
        assert_ne!(first.salt, second.salt);
        assert_ne!(first.hash, second.hash);
        assert!(!first.is_outdated());
        let json = serde_json::to_string(&first).unwrap();
        assert!(!json.contains("a long 密码 123"));
        let loaded: Verifier = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.cost, Some(COST));
        assert!(loaded.verify("a long 密码 123").unwrap());
        assert!(!loaded.verify("wrong password").unwrap());
        assert!(!loaded.verify("a long 密码 123 ").unwrap());
    }

    #[test]
    fn a_version_1_verifier_still_verifies_and_is_outdated() {
        let json = serde_json::to_string(&first_version("old password")).unwrap();
        assert!(!json.contains("cost"));
        let loaded: Verifier = serde_json::from_str(&json).unwrap();
        loaded.validate().unwrap();
        assert!(loaded.is_outdated());
        assert!(loaded.verify("old password").unwrap());
        assert!(!loaded.verify("new password").unwrap());
    }

    #[test]
    fn invalid_or_unknown_verifiers_are_rejected() {
        assert!(serde_json::from_str::<Verifier>("broken").is_err());
        let verifier = Verifier::new("password123").unwrap();
        let broken = [
            Verifier {
                version: 3,
                ..verifier.clone()
            },
            Verifier {
                cost: None,
                ..verifier.clone()
            },
            Verifier {
                salt: vec![0; 8],
                ..verifier.clone()
            },
            Verifier {
                hash: vec![0; 16],
                ..verifier.clone()
            },
            Verifier {
                cost: Some(super::Cost {
                    memory_kib: u32::MAX,
                    ..COST
                }),
                ..verifier
            },
        ];
        for verifier in broken {
            assert!(verifier.validate().is_err());
            assert!(verifier.verify("password123").is_err());
        }
    }
}
