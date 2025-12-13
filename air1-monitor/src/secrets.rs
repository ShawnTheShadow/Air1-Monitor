use anyhow::{Context, Result};

const SERVICE_NAME: &str = "com.air1.monitor";
const ACCOUNT_NAME: &str = "air1-mqtt";

pub fn load_password() -> Result<Option<String>> {
    let entry =
        keyring::Entry::new(SERVICE_NAME, ACCOUNT_NAME).context("failed to open keyring entry")?;
    match entry.get_password() {
        Ok(secret) => Ok(Some(secret)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(err) => Err(err).context("failed to read password from keyring"),
    }
}

pub fn save_password(secret: &str) -> Result<()> {
    let entry =
        keyring::Entry::new(SERVICE_NAME, ACCOUNT_NAME).context("failed to open keyring entry")?;
    entry
        .set_password(secret)
        .context("failed to write password to keyring")
}

pub fn delete_password() -> Result<()> {
    let entry =
        keyring::Entry::new(SERVICE_NAME, ACCOUNT_NAME).context("failed to open keyring entry")?;
    match entry.delete_password() {
        Ok(_) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(err) => Err(err).context("failed to delete password from keyring"),
    }
}
