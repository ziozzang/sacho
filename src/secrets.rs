//! Generic encrypted local inputs. No source/provider-specific credentials.
use anyhow::{Context, Result, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead, AeadCore, KeyInit, OsRng, Payload},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::Path,
};
use zeroize::Zeroizing;
const LIMIT: usize = 262144;
const AAD: &[u8] = b"sacho-secret-vault-v1";
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    version: u8,
    nonce: String,
    ciphertext: String,
}
pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        && !name.starts_with("SACHO_PROBE_")
        && name != "PATH"
        && name != "LANG"
}
fn private_file(path: &Path) -> Result<File> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .context("cannot open private vault/key file")?;
    ensure!(
        file.metadata()?.is_file() && file.metadata()?.permissions().mode() & 0o077 == 0,
        "vault and key files must be regular files without group/other permissions"
    );
    Ok(file)
}
fn read(file: &mut File, limit: usize) -> Result<Zeroizing<Vec<u8>>> {
    let mut data = Zeroizing::new(Vec::new());
    file.take(limit as u64 + 1).read_to_end(&mut data)?;
    ensure!(data.len() <= limit, "vault/key file exceeds size limit");
    Ok(data)
}
fn locked_key(path: &Path, exclusive: bool) -> Result<(File, Zeroizing<Vec<u8>>)> {
    use std::os::fd::AsRawFd;
    let mut file = private_file(path)?;
    let mode = if exclusive {
        libc::LOCK_EX
    } else {
        libc::LOCK_SH
    };
    ensure!(
        unsafe { libc::flock(file.as_raw_fd(), mode) } == 0,
        "cannot lock vault key file"
    );
    let data = read(&mut file, 256)?;
    let key = Zeroizing::new(
        STANDARD
            .decode(std::str::from_utf8(&data)?.trim())
            .context("invalid vault key encoding")?,
    );
    ensure!(key.len() == 32, "vault key must be 32 random bytes");
    Ok((file, key))
}
fn decrypt(vault: &Path, key: &[u8]) -> Result<BTreeMap<String, String>> {
    let data = read(&mut private_file(vault)?, LIMIT * 2)?;
    let envelope: Envelope =
        serde_json::from_slice(&data).context("invalid encrypted vault envelope")?;
    ensure!(envelope.version == 1, "unsupported vault version");
    let nonce = STANDARD
        .decode(envelope.nonce)
        .context("invalid vault nonce")?;
    ensure!(nonce.len() == 24, "invalid vault nonce length");
    let ciphertext = STANDARD
        .decode(envelope.ciphertext)
        .context("invalid vault ciphertext")?;
    let cipher =
        XChaCha20Poly1305::new_from_slice(key).map_err(|_| anyhow::anyhow!("invalid vault key"))?;
    let plain = Zeroizing::new(
        cipher
            .decrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: &ciphertext,
                    aad: AAD,
                },
            )
            .map_err(|_| {
                anyhow::anyhow!("vault authentication failed (wrong key or modified file)")
            })?,
    );
    ensure!(plain.len() <= LIMIT, "vault plaintext exceeds size limit");
    let values: BTreeMap<String, String> = serde_json::from_slice(&plain)
        .map_err(|_| anyhow::anyhow!("invalid decrypted vault data"))?;
    ensure!(
        values
            .iter()
            .all(|(name, value)| valid_name(name) && value.len() <= 65536 && !value.contains('\0')),
        "invalid vault entry"
    );
    Ok(values)
}
pub fn load(vault: &Path, key_file: &Path) -> Result<BTreeMap<String, String>> {
    let (_lock, key) = locked_key(key_file, false)?;
    decrypt(vault, &key)
}
fn encrypt(values: &BTreeMap<String, String>, key: &[u8]) -> Result<Vec<u8>> {
    let plain = Zeroizing::new(serde_json::to_vec(values)?);
    ensure!(
        plain.len() <= LIMIT,
        "vault exceeds 256 KiB plaintext limit"
    );
    let cipher =
        XChaCha20Poly1305::new_from_slice(key).map_err(|_| anyhow::anyhow!("invalid vault key"))?;
    let nonce = XChaCha20Poly1305::generate_nonce(&mut OsRng);
    let ciphertext = cipher
        .encrypt(
            &nonce,
            Payload {
                msg: &plain,
                aad: AAD,
            },
        )
        .map_err(|_| anyhow::anyhow!("vault encryption failed"))?;
    Ok(serde_json::to_vec(&Envelope {
        version: 1,
        nonce: STANDARD.encode(nonce),
        ciphertext: STANDARD.encode(ciphertext),
    })?)
}
fn create(path: &Path, data: &[u8]) -> Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(data)?;
    file.sync_all()?;
    Ok(())
}
pub fn init(vault: &Path, key_file: &Path) -> Result<()> {
    ensure!(
        !vault.exists() && !key_file.exists(),
        "vault/key already exists; init never overwrites"
    );
    ensure!(vault != key_file, "vault and key must be separate files");
    let key = XChaCha20Poly1305::generate_key(&mut OsRng);
    let encoded = Zeroizing::new(STANDARD.encode(key));
    create(key_file, encoded.as_bytes())?;
    if let Err(error) = (|| {
        let (_lock, key) = locked_key(key_file, true)?;
        create(vault, &encrypt(&BTreeMap::new(), &key)?)
    })() {
        let _ = std::fs::remove_file(key_file);
        return Err(error);
    }
    Ok(())
}
pub fn update(vault: &Path, key_file: &Path, name: &str, value: Option<String>) -> Result<()> {
    ensure!(valid_name(name), "invalid secret name");
    if let Some(value) = &value {
        ensure!(
            value.len() <= 65536 && !value.contains('\0'),
            "secret value exceeds limit or contains NUL"
        );
    }
    let (_lock, key) = locked_key(key_file, true)?;
    let mut values = decrypt(vault, &key)?;
    if let Some(value) = value {
        values.insert(name.into(), value);
    } else {
        ensure!(values.remove(name).is_some(), "secret name not found");
    }
    let data = encrypt(&values, &key)?;
    let parent = vault
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let temp = parent.join(format!(".sacho-vault-{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        create(&temp, &data)?;
        std::fs::rename(&temp, vault)?;
        File::open(parent)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}
