use sacho::{config::Config, secrets};
use std::fs;
#[test]
fn encrypted_roundtrip_has_no_plain_values_and_fresh_nonces() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let vault = directory.path().join("secrets.enc");
    let key = directory.path().join("unlock.key");
    secrets::init(&vault, &key)?;
    secrets::update(
        &vault,
        &key,
        "INPUT",
        Some("plaintext-sentinel-not-for-storage".into()),
    )?;
    let first = fs::read(&vault)?;
    assert!(!String::from_utf8_lossy(&first).contains("plaintext-sentinel-not-for-storage"));
    assert_eq!(
        secrets::load(&vault, &key)?["INPUT"],
        "plaintext-sentinel-not-for-storage"
    );
    secrets::update(
        &vault,
        &key,
        "INPUT",
        Some("plaintext-sentinel-not-for-storage".into()),
    )?;
    assert_ne!(first, fs::read(&vault)?);
    secrets::update(&vault, &key, "INPUT", None)?;
    assert!(secrets::load(&vault, &key)?.is_empty());
    assert!(secrets::init(&vault, &key).is_err());
    Ok(())
}
#[test]
fn wrong_key_modified_ciphertext_and_invalid_names_are_rejected() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let vault = directory.path().join("vault");
    let key = directory.path().join("key");
    let other = directory.path().join("other");
    secrets::init(&vault, &key)?;
    secrets::init(&directory.path().join("other-vault"), &other)?;
    assert!(secrets::load(&vault, &other).is_err());
    for name in ["PATH", "SACHO_PROBE_TIMEOUT", "bad=name"] {
        assert!(secrets::update(&vault, &key, name, Some("value".into())).is_err());
    }
    let mut envelope: serde_json::Value = serde_json::from_slice(&fs::read(&vault)?)?;
    envelope["ciphertext"] = serde_json::json!("AAAAAAAAAAAAAAAAAAAAAA==");
    fs::write(&vault, serde_json::to_vec(&envelope)?)?;
    assert!(secrets::load(&vault, &key).is_err());
    Ok(())
}
#[test]
fn permissive_files_and_symlinks_are_rejected() -> anyhow::Result<()> {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let directory = tempfile::tempdir()?;
    let vault = directory.path().join("vault");
    let key = directory.path().join("key");
    secrets::init(&vault, &key)?;
    fs::set_permissions(&key, fs::Permissions::from_mode(0o644))?;
    assert!(secrets::load(&vault, &key).is_err());
    fs::set_permissions(&key, fs::Permissions::from_mode(0o600))?;
    let link = directory.path().join("key-link");
    symlink(&key, &link)?;
    assert!(secrets::load(&vault, &link).is_err());
    fs::set_permissions(&vault, fs::Permissions::from_mode(0o644))?;
    assert!(secrets::load(&vault, &key).is_err());
    Ok(())
}
#[test]
fn decrypted_values_never_serialize_with_config_and_environment_can_override() -> anyhow::Result<()>
{
    let mut config = Config::default();
    config
        .secret_values
        .insert("INPUT".into(), "hidden-sentinel".into());
    config
        .secret_values
        .insert("PATH".into(), "not-the-path".into());
    assert_eq!(config.environment("INPUT")?, "hidden-sentinel");
    assert_eq!(config.environment("PATH")?, std::env::var("PATH")?);
    assert!(!serde_json::to_string(&config)?.contains("hidden-sentinel"));
    assert!(!toml::to_string(&config)?.contains("hidden-sentinel"));
    Ok(())
}
#[test]
fn concurrent_updates_preserve_both_entries() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let vault = directory.path().join("vault");
    let key = directory.path().join("key");
    secrets::init(&vault, &key)?;
    let handles: Vec<_> = (0..8)
        .map(|i| {
            let vault = vault.clone();
            let key = key.clone();
            std::thread::spawn(move || {
                secrets::update(&vault, &key, &format!("INPUT_{i}"), Some(i.to_string())).unwrap()
            })
        })
        .collect();
    for handle in handles {
        handle.join().unwrap();
    }
    assert_eq!(secrets::load(&vault, &key)?.len(), 8);
    Ok(())
}
