use anyhow::{Context, Result};
use tracing::{debug, warn};

const SERVICE_NAME: &str = "com.air1.monitor";
const ACCOUNT_NAME: &str = "air1-mqtt";

fn open_entry() -> Result<keyring::Entry> {
    keyring::Entry::new(SERVICE_NAME, ACCOUNT_NAME)
        .with_context(|| "failed to access system keyring (Entry::new)")
}

/// Return true if the system keyring appears usable.
pub fn keyring_available() -> bool {
    match open_entry() {
        Ok(_) => true,
        Err(err) => {
            debug!("keyring not available: {:#}", err);
            false
        }
    }
}

pub fn load_password() -> Result<Option<String>> {
    let entry = open_entry().context("failed to open keyring entry")?;
    match entry.get_password() {
        Ok(secret) => Ok(Some(secret)),
        Err(keyring::Error::NoEntry) => {
            debug!("no password stored in keyring");
            Ok(None)
        }
        Err(err) => {
            warn!("failed to read password from keyring: {:#}", err);
            Err(err).context("failed to read password from keyring")
        }
    }
}

pub fn save_password(secret: &str) -> Result<()> {
    let entry = open_entry().context("failed to open keyring entry")?;
    entry
        .set_password(secret)
        .with_context(|| "failed to write password to keyring")
        .map(|_| ())
}

pub fn delete_password() -> Result<()> {
    let entry = open_entry().context("failed to open keyring entry")?;
    match entry.delete_credential() {
        Ok(_) => Ok(()),
        Err(keyring::Error::NoEntry) => {
            debug!("no keyring entry to delete");
            Ok(())
        }
        Err(err) => {
            warn!("failed to delete password from keyring: {:#}", err);
            Err(err).context("failed to delete password from keyring")
        }
    }
}
