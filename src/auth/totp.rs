//! TOTP code generation for the adAS 2FA step. Replicates otplib's base32
//! SHA1 6-digit 30s-period generation. The JS carried a custom
//! MAX_SECRET_BYTES=256 guardrail override because UVUS issues secrets longer
//! than otplib's default 64-byte RFC cap; totp-rs doesn't enforce that cap at
//! all, so there's nothing to override here.

use anyhow::{Context, Result};
use totp_rs::{Algorithm, Builder, Secret};

/// Strip everything but alphanumerics and uppercase, same as auth.mjs's
/// sanitizeTotpSecret (UVUS secrets sometimes come with spaces/dashes).
fn sanitize(secret: &str) -> String {
    secret
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_uppercase()
}

pub fn generate_code(secret: &str) -> Result<String> {
    let sanitized = sanitize(secret);
    let secret = Secret::try_from_base32(&sanitized)
        .map_err(|e| anyhow::anyhow!("invalid base32 TOTP secret: {e:?}"))?;
    let totp = Builder::new()
        .with_algorithm(Algorithm::SHA1)
        .with_digits(6)
        .with_skew(0)
        .with_step_duration(30)
        .with_secret(secret)
        .build()
        .context("constructing TOTP generator")?;
    Ok(totp.generate_current().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_strips_non_alnum_and_uppercases() {
        assert_eq!(sanitize("jb sw-y3d p"), "JBSWY3DP");
    }

    #[test]
    fn generate_code_produces_six_digits() {
        // RFC 4648 test secret, base32 for "12345678901234567890"
        let code = generate_code("GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ").unwrap();
        assert_eq!(code.len(), 6);
        assert!(code.chars().all(|c| c.is_ascii_digit()));
    }
}
