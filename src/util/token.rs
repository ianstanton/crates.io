use diesel::{deserialize::FromSql, pg::Pg, serialize::ToSql, sql_types::Bytea};
use rand::{distributions::Uniform, rngs::OsRng, Rng};
use sha2::{Digest, Sha256};

const TOKEN_LENGTH: usize = 32;

#[derive(FromSqlRow, AsExpression, Clone, PartialEq, Eq)]
#[diesel(sql_type = Bytea)]
pub struct SecureToken {
    sha256: Vec<u8>,
}

impl SecureToken {
    pub(crate) fn generate(kind: SecureTokenKind) -> NewSecureToken {
        let plaintext = format!(
            "{}{}",
            kind.prefix(),
            generate_secure_alphanumeric_string(TOKEN_LENGTH)
        );
        let sha256 = Self::hash(&plaintext);

        NewSecureToken {
            plaintext,
            inner: Self { sha256 },
        }
    }

    pub(crate) fn parse(kind: SecureTokenKind, plaintext: &str) -> Option<Self> {
        // This will both reject tokens without a prefix and tokens of the wrong kind.
        if SecureTokenKind::from_token(plaintext) != Some(kind) {
            return None;
        }

        let sha256 = Self::hash(plaintext);
        Some(Self { sha256 })
    }

    pub fn hash(plaintext: &str) -> Vec<u8> {
        Sha256::digest(plaintext.as_bytes()).as_slice().to_vec()
    }
}

impl std::fmt::Debug for SecureToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SecureToken")
    }
}

impl ToSql<Bytea, Pg> for SecureToken {
    fn to_sql(&self, out: &mut diesel::serialize::Output<'_, '_, Pg>) -> diesel::serialize::Result {
        ToSql::<Bytea, Pg>::to_sql(&self.sha256, &mut out.reborrow())
    }
}

impl FromSql<Bytea, Pg> for SecureToken {
    fn from_sql(bytes: diesel::pg::PgValue<'_>) -> diesel::deserialize::Result<Self> {
        Ok(Self {
            sha256: FromSql::<Bytea, Pg>::from_sql(bytes)?,
        })
    }
}

pub(crate) struct NewSecureToken {
    plaintext: String,
    inner: SecureToken,
}

impl NewSecureToken {
    pub(crate) fn plaintext(&self) -> &str {
        &self.plaintext
    }

    #[cfg(test)]
    pub(crate) fn into_inner(self) -> SecureToken {
        self.inner
    }
}

impl std::ops::Deref for NewSecureToken {
    type Target = SecureToken;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

fn generate_secure_alphanumeric_string(len: usize) -> String {
    const CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";

    OsRng
        .sample_iter(Uniform::from(0..CHARS.len()))
        .map(|idx| CHARS[idx] as char)
        .take(len)
        .collect()
}

macro_rules! secure_token_kind {
    ($(#[$attr:meta])* $vis:vis enum $name:ident { $($key:ident => $repr:expr,)* }) => {
        $(#[$attr])*
        $vis enum $name {
            $($key,)*
        }

        impl $name {
            const VARIANTS: &'static [Self] = &[$(Self::$key,)*];

            fn prefix(&self) -> &'static str {
                match self {
                    $(Self::$key => $repr,)*
                }
            }
        }
    }
}

secure_token_kind! {
    /// Represents every kind of secure token generated by crates.io. When you need to generate a
    /// new kind of token you should also add its own kind with its own unique prefix.
    ///
    /// NEVER CHANGE THE PREFIX OF EXISTING TOKEN TYPES!!! Doing so will implicitly revoke all the
    /// tokens of that kind, disrupting production users.
    #[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
    pub(crate) enum SecureTokenKind {
        Api => "cio", // Crates.IO
    }
}

impl SecureTokenKind {
    fn from_token(token: &str) -> Option<Self> {
        Self::VARIANTS
            .iter()
            .find(|v| token.starts_with(v.prefix()))
            .copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn test_generated_and_parse() {
        const KIND: SecureTokenKind = SecureTokenKind::Api;

        let token = SecureToken::generate(KIND);
        assert!(token.plaintext().starts_with(KIND.prefix()));
        assert_eq!(
            token.sha256,
            Sha256::digest(token.plaintext().as_bytes()).as_slice()
        );

        let parsed =
            SecureToken::parse(KIND, token.plaintext()).expect("failed to parse back the token");
        assert_eq!(parsed.sha256, token.sha256);
    }

    #[test]
    fn test_parse_no_kind() {
        assert!(SecureToken::parse(SecureTokenKind::Api, "nokind").is_none());
    }

    #[test]
    fn test_persistent_prefixes() {
        // Changing prefixes will implicitly revoke all the tokens of that kind, disrupting users.
        // This test serves as a reminder for future maintainers not to change the prefixes, and
        // to ensure all the variants are tested by this test.
        let mut remaining: HashSet<_> = SecureTokenKind::VARIANTS.iter().copied().collect();
        let mut ensure = |kind: SecureTokenKind, prefix| {
            assert_eq!(kind.prefix(), prefix);
            remaining.remove(&kind);
        };

        ensure(SecureTokenKind::Api, "cio");

        assert!(
            remaining.is_empty(),
            "not all variants have a test to check the prefix"
        );
    }

    #[test]
    fn test_conflicting_prefixes() {
        // This sanity check prevents multiple tokens from starting with the same prefix, which
        // would mess up the token kind detection. If this test fails after adding another variant
        // do not change the test, choose another prefix instead.
        for variant in SecureTokenKind::VARIANTS {
            for other in SecureTokenKind::VARIANTS {
                if variant == other {
                    continue;
                }
                if variant.prefix().starts_with(other.prefix()) {
                    panic!("variants {variant:?} and {other:?} share a prefix");
                }
            }
        }
    }
}
