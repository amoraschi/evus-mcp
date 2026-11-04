//! One-off CLI to store UVUS creds in OS keychain. Equivalent to set-creds.mjs.

use anyhow::Result;

pub fn run() -> Result<()> {
    crate::credentials::prompt_and_store()
}
