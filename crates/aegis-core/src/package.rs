use crate::error::{AegisError, Result};
use ring::signature::{self, KeyPair};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{Cursor, Read, Write};
use std::path::{Component, Path, PathBuf};

/// Current AegisCMS Core version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Validates that the current AegisCMS version meets the required minimum version.
pub fn check_min_aegis_version(min_ver: &str) -> Result<()> {
    let req = semver::Version::parse(min_ver.trim()).map_err(|e| {
        AegisError::Validation(format!(
            "Invalid 'min_aegis_version' ({:?}): {}",
            min_ver, e
        ))
    })?;

    let cur = semver::Version::parse(VERSION).map_err(|e| {
        AegisError::Internal(format!(
            "Failed to parse current Aegis version ({}): {}",
            VERSION, e
        ))
    })?;

    if cur < req {
        return Err(AegisError::Validation(format!(
            "Package requires Aegis version >= {}, but current running version is {}",
            req, cur
        )));
    }

    Ok(())
}

/// Package signature information stored in `signature.json`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PackageSignature {
    /// Digital signature algorithm (currently only "ed25519" is supported)
    pub algorithm: String,
    /// 32-byte Ed25519 public key in hexadecimal representation
    pub public_key: String,
    /// 64-byte Ed25519 signature in hexadecimal representation
    pub signature: String,
    /// Signature target ("package", defaults to "package")
    #[serde(default = "default_signature_target")]
    pub target: String,
}

fn default_signature_target() -> String {
    "package".to_string()
}

/// Validates `[plugins]` / `[theme]` signature policy settings (`require_signatures` and `trusted_public_keys`).
pub fn validate_signature_policy(
    section: &str,
    require_signatures: bool,
    trusted_public_keys: &[String],
) -> Result<()> {
    if require_signatures && trusted_public_keys.is_empty() {
        return Err(AegisError::Config(format!(
            "[{}].trusted_public_keys must not be empty when [{}].require_signatures = true",
            section, section
        )));
    }
    for key in trusted_public_keys {
        let trimmed = key.trim();
        let bytes = hex::decode(trimmed).map_err(|e| {
            AegisError::Config(format!(
                "[{}].trusted_public_keys: invalid hex public key {:?}: {}",
                section, key, e
            ))
        })?;
        if bytes.len() != 32 {
            return Err(AegisError::Config(format!(
                "[{}].trusted_public_keys: public key {:?} must be 32 bytes (64 hex characters), got {} bytes",
                section,
                key,
                bytes.len()
            )));
        }
    }
    Ok(())
}

/// Computes the canonical package digest over `(relative_path, content)` entries (excluding `signature.json`).
/// Entries are sorted by their normalized slash-separated relative paths.
pub fn compute_package_entries_digest<'a, I>(entries: I) -> Vec<u8>
where
    I: IntoIterator<Item = (&'a str, &'a [u8])>,
{
    let mut sorted: Vec<(&'a str, &'a [u8])> = entries
        .into_iter()
        .filter(|(rel_path, _)| *rel_path != "signature.json")
        .collect();
    sorted.sort_by(|a, b| a.0.cmp(b.0));

    let mut hasher = Sha256::new();
    for (rel_path, content) in sorted {
        let file_hash = Sha256::digest(content);
        hasher.update(rel_path.as_bytes());
        hasher.update(b":");
        hasher.update(file_hash);
        hasher.update(b"\n");
    }

    hasher.finalize().to_vec()
}

/// Computes the canonical package digest over files in `package_dir` (excluding signature.json).
/// Files are sorted by their normalized slash-separated relative paths.
pub fn compute_package_files_digest(package_dir: &Path) -> Result<Vec<u8>> {
    let mut files = Vec::new();
    collect_files_recursive(package_dir, package_dir, &mut files)?;
    files.sort();

    let mut loaded = Vec::with_capacity(files.len());
    for (rel_path, file_path) in files {
        if rel_path == "signature.json" {
            continue;
        }
        let content = std::fs::read(&file_path).map_err(AegisError::Io)?;
        loaded.push((rel_path, content));
    }

    Ok(compute_package_entries_digest(
        loaded
            .iter()
            .map(|(rel, bytes)| (rel.as_str(), bytes.as_slice())),
    ))
}

fn collect_files_recursive(
    base: &Path,
    current: &Path,
    list: &mut Vec<(String, PathBuf)>,
) -> Result<()> {
    if !current.is_dir() {
        return Ok(());
    }
    for entry in std::fs::read_dir(current).map_err(AegisError::Io)? {
        let entry = entry.map_err(AegisError::Io)?;
        let path = entry.path();
        if path.is_dir() {
            collect_files_recursive(base, &path, list)?;
        } else if path.is_file() {
            let rel = path
                .strip_prefix(base)
                .map_err(|e| AegisError::Internal(e.to_string()))?;
            let normalized = rel
                .components()
                .filter_map(|c| match c {
                    Component::Normal(s) => s.to_str(),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("/");
            list.push((normalized, path));
        }
    }
    Ok(())
}

/// Verifies a package's signature in `package_dir` if present or required.
/// Returns the verified public key hex string, or `None` if signature was optional and absent.
pub fn verify_package_signature(
    package_dir: &Path,
    trusted_keys: Option<&[String]>,
    require_signature: bool,
) -> Result<Option<String>> {
    let has_trusted_keys = trusted_keys.is_some_and(|keys| !keys.is_empty());
    if require_signature && !has_trusted_keys {
        return Err(AegisError::Validation(
            "Package signature required by policy, but no trusted public keys are configured"
                .to_string(),
        ));
    }

    let sig_file = package_dir.join("signature.json");
    if !sig_file.exists() {
        if require_signature {
            return Err(AegisError::Validation(
                "Package signature required by policy, but 'signature.json' was not found"
                    .to_string(),
            ));
        }
        return Ok(None);
    }

    let sig_content = std::fs::read_to_string(&sig_file).map_err(AegisError::Io)?;
    let sig: PackageSignature = serde_json::from_str(&sig_content)
        .map_err(|e| AegisError::Validation(format!("Invalid 'signature.json' manifest: {}", e)))?;

    if sig.algorithm.to_lowercase() != "ed25519" {
        return Err(AegisError::Validation(format!(
            "Unsupported package signature algorithm '{}' (only 'ed25519' is supported)",
            sig.algorithm
        )));
    }

    let pk_hex = sig.public_key.trim();
    let pk_bytes = hex::decode(pk_hex).map_err(|e| {
        AegisError::Validation(format!("Invalid public key hex in signature: {}", e))
    })?;
    if pk_bytes.len() != 32 {
        return Err(AegisError::Validation(format!(
            "Invalid public key length in signature (expected 32 bytes, got {})",
            pk_bytes.len()
        )));
    }

    let sig_hex = sig.signature.trim();
    let sig_bytes = hex::decode(sig_hex).map_err(|e| {
        AegisError::Validation(format!("Invalid signature hex in signature: {}", e))
    })?;
    if sig_bytes.len() != 64 {
        return Err(AegisError::Validation(format!(
            "Invalid signature length in signature (expected 64 bytes, got {})",
            sig_bytes.len()
        )));
    }

    // Check trusted keys if configured
    if let Some(keys) = trusted_keys {
        if !keys.is_empty() {
            let matches_trusted = keys.iter().any(|k| k.trim().eq_ignore_ascii_case(pk_hex));
            if !matches_trusted {
                return Err(AegisError::Validation(format!(
                    "Package signature public key '{}' is not in trusted public keys list",
                    pk_hex
                )));
            }
        }
    }

    // Prepare target payload to verify (only "package" is supported)
    let target_data = match sig.target.as_str() {
        "package" => compute_package_files_digest(package_dir)?,
        other => {
            return Err(AegisError::Validation(format!(
                "Unsupported signature target '{}' (only 'package' is supported)",
                other
            )))
        }
    };

    let peer_public_key = signature::UnparsedPublicKey::new(&signature::ED25519, &pk_bytes);
    peer_public_key
        .verify(&target_data, &sig_bytes)
        .map_err(|_| {
            AegisError::Validation(
                "Package digital signature verification failed: invalid signature".to_string(),
            )
        })?;

    Ok(Some(pk_hex.to_string()))
}

/// Signs package digest using an Ed25519 key pair for tests/tooling.
pub fn create_test_signature(
    target_data: &[u8],
    key_pair: &signature::Ed25519KeyPair,
    target: &str,
) -> PackageSignature {
    let sig = key_pair.sign(target_data);
    PackageSignature {
        algorithm: "ed25519".to_string(),
        public_key: hex::encode(key_pair.public_key().as_ref()),
        signature: hex::encode(sig.as_ref()),
        target: target.to_string(),
    }
}

/// Options for safely extracting ZIP archives.
#[derive(Debug, Clone)]
pub struct ZipExtractOptions {
    /// Maximum uncompressed size for a single file (default: 50 MiB)
    pub max_file_size: u64,
    /// Maximum total uncompressed size for the entire archive (default: 100 MiB)
    pub max_total_size: u64,
    /// Maximum number of file entries allowed in archive (default: 1,000)
    pub max_entries: usize,
}

impl Default for ZipExtractOptions {
    fn default() -> Self {
        Self {
            max_file_size: 50 * 1024 * 1024,
            max_total_size: 100 * 1024 * 1024,
            max_entries: 1000,
        }
    }
}

/// Safely extracts a ZIP archive into `dest_dir`.
/// Prevents ZipSlip directory traversal, symlink attacks, and zip bombs.
pub fn extract_zip_safely(
    zip_bytes: &[u8],
    dest_dir: &Path,
    options: Option<ZipExtractOptions>,
) -> Result<Vec<PathBuf>> {
    let opts = options.unwrap_or_default();
    let reader = Cursor::new(zip_bytes);
    let mut archive = zip::ZipArchive::new(reader)
        .map_err(|e| AegisError::Validation(format!("Failed to parse ZIP archive: {}", e)))?;

    if archive.len() > opts.max_entries {
        return Err(AegisError::Validation(format!(
            "ZIP archive contains too many entries ({} > limit {})",
            archive.len(),
            opts.max_entries
        )));
    }

    std::fs::create_dir_all(dest_dir).map_err(AegisError::Io)?;
    let canonical_dest = dest_dir.canonicalize().map_err(AegisError::Io)?;

    let mut total_extracted_size: u64 = 0;
    let mut extracted_paths = Vec::new();

    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| {
            AegisError::Validation(format!("Failed to read ZIP entry #{}: {}", i, e))
        })?;

        let file_name = file
            .name()
            .map_err(|e| {
                AegisError::Validation(format!("Failed to read ZIP entry #{} name: {}", i, e))
            })?
            .to_string();

        // 1. Enclosed name validation against ZipSlip
        let enclosed = match file.enclosed_name() {
            Some(path) => path.to_owned(),
            None => {
                return Err(AegisError::Validation(format!(
                    "Malicious ZIP path detected: {:?}",
                    file_name
                )));
            }
        };

        // Extra check on path components
        for comp in enclosed.components() {
            match comp {
                Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                    return Err(AegisError::Validation(format!(
                        "Illegal path component in ZIP entry: {:?}",
                        file_name
                    )));
                }
                _ => {}
            }
        }

        let outpath = dest_dir.join(&enclosed);

        if file.is_dir() {
            std::fs::create_dir_all(&outpath).map_err(AegisError::Io)?;
        } else {
            // Verify single file size limit
            if file.size() > opts.max_file_size {
                return Err(AegisError::Validation(format!(
                    "File '{}' exceeds maximum allowed size ({} > {})",
                    file_name,
                    file.size(),
                    opts.max_file_size
                )));
            }

            if let Some(p) = outpath.parent() {
                if !p.exists() {
                    std::fs::create_dir_all(p).map_err(AegisError::Io)?;
                }
            }

            let mut outfile = File::create(&outpath).map_err(AegisError::Io)?;
            let mut buffer = [0u8; 8192];
            let mut file_extracted: u64 = 0;

            loop {
                let n = file.read(&mut buffer).map_err(AegisError::Io)?;
                if n == 0 {
                    break;
                }
                file_extracted += n as u64;
                total_extracted_size += n as u64;

                if file_extracted > opts.max_file_size {
                    return Err(AegisError::Validation(format!(
                        "File '{}' exceeded maximum allowed size during extraction",
                        file_name
                    )));
                }
                if total_extracted_size > opts.max_total_size {
                    return Err(AegisError::Validation(format!(
                        "Total archive size exceeded maximum allowed limit ({} bytes)",
                        opts.max_total_size
                    )));
                }

                outfile.write_all(&buffer[..n]).map_err(AegisError::Io)?;
            }

            // Verify the created file is strictly inside the destination canonical root
            if let Ok(canon_out) = outpath.canonicalize() {
                if !canon_out.starts_with(&canonical_dest) {
                    return Err(AegisError::Validation(format!(
                        "Extracted file outside destination directory: {:?}",
                        outpath
                    )));
                }
            }

            extracted_paths.push(outpath);
        }
    }

    Ok(extracted_paths)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_min_aegis_version() {
        // Less than or equal to current version should succeed
        assert!(check_min_aegis_version("0.1.0").is_ok());
        assert!(check_min_aegis_version("0.0.1").is_ok());

        // Higher version should fail
        let err = check_min_aegis_version("99.0.0");
        assert!(err.is_err());
        assert!(err
            .unwrap_err()
            .to_string()
            .contains("Package requires Aegis version >="));

        // Malformed semver should fail
        let err = check_min_aegis_version("not-a-semver");
        assert!(err.is_err());
    }

    #[test]
    fn test_signature_roundtrip_package_and_policy() {
        let rng = ring::rand::SystemRandom::new();
        let pkcs8 = signature::Ed25519KeyPair::generate_pkcs8(&rng).unwrap();
        let key_pair = signature::Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).unwrap();

        let temp_dir = std::env::temp_dir().join(format!("aegis_sig_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        let manifest_data = br#"{"name": "test-plugin", "version": "1.0.0"}"#;
        let wasm_data = b"\0asm\x01\0\0\0";
        std::fs::write(temp_dir.join("plugin.manifest.json"), manifest_data).unwrap();
        std::fs::write(temp_dir.join("plugin.wasm"), wasm_data).unwrap();

        let pkg_digest = compute_package_files_digest(&temp_dir).unwrap();
        let sig = create_test_signature(&pkg_digest, &key_pair, "package");

        // Write signature omitting `target` to verify default is "package"
        let sig_json = serde_json::json!({
            "algorithm": sig.algorithm,
            "public_key": sig.public_key,
            "signature": sig.signature,
        });
        std::fs::write(
            temp_dir.join("signature.json"),
            serde_json::to_string_pretty(&sig_json).unwrap(),
        )
        .unwrap();

        // 1. require_signature = true without trusted keys is rejected
        let err = verify_package_signature(&temp_dir, None, true);
        assert!(err.is_err());
        assert!(err
            .unwrap_err()
            .to_string()
            .contains("no trusted public keys are configured"));

        let err = verify_package_signature(&temp_dir, Some(&[]), true);
        assert!(err.is_err());
        assert!(err
            .unwrap_err()
            .to_string()
            .contains("no trusted public keys are configured"));

        // 2. Valid signature with require_signature = false and no trusted keys restriction
        let verified_pk = verify_package_signature(&temp_dir, None, false).unwrap();
        assert_eq!(verified_pk, Some(sig.public_key.clone()));

        // 3. Valid signature with require_signature = true and matching trusted key
        let verified_pk =
            verify_package_signature(&temp_dir, Some(std::slice::from_ref(&sig.public_key)), true)
                .unwrap();
        assert_eq!(verified_pk, Some(sig.public_key.clone()));

        // 4. Untrusted key rejection
        let err = verify_package_signature(
            &temp_dir,
            Some(&["0000000000000000000000000000000000000000000000000000000000000000".to_string()]),
            true,
        );
        assert!(err.is_err());
        assert!(err
            .unwrap_err()
            .to_string()
            .contains("not in trusted public keys list"));

        // 5. Tampered Wasm file rejection (protects non-manifest files)
        std::fs::write(temp_dir.join("plugin.wasm"), b"\0asm\x01\0\0\x99").unwrap();
        let err =
            verify_package_signature(&temp_dir, Some(std::slice::from_ref(&sig.public_key)), true);
        assert!(err.is_err());
        assert!(err
            .unwrap_err()
            .to_string()
            .contains("digital signature verification failed"));
        std::fs::write(temp_dir.join("plugin.wasm"), wasm_data).unwrap();

        // 6. Legacy "manifest" target is rejected
        let manifest_sig = create_test_signature(manifest_data, &key_pair, "manifest");
        std::fs::write(
            temp_dir.join("signature.json"),
            serde_json::to_string_pretty(&manifest_sig).unwrap(),
        )
        .unwrap();
        let err =
            verify_package_signature(&temp_dir, Some(std::slice::from_ref(&sig.public_key)), true);
        assert!(err.is_err());
        assert!(err
            .unwrap_err()
            .to_string()
            .contains("Unsupported signature target 'manifest'"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_validate_signature_policy() {
        let valid_key = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        assert!(validate_signature_policy("plugins", false, &[]).is_ok());
        assert!(validate_signature_policy("plugins", false, &[valid_key.to_string()]).is_ok());
        assert!(validate_signature_policy("plugins", true, &[valid_key.to_string()]).is_ok());

        // require_signatures = true with empty trusted_public_keys must fail
        let err = validate_signature_policy("plugins", true, &[]);
        assert!(err.is_err());
        assert!(err
            .unwrap_err()
            .to_string()
            .contains("[plugins].trusted_public_keys must not be empty"));

        // Invalid hex or wrong length must fail
        assert!(validate_signature_policy("theme", false, &["not-hex".to_string()]).is_err());
        assert!(validate_signature_policy("theme", false, &["012345".to_string()]).is_err());
    }

    #[test]
    fn test_extract_zip_safely_normal() {
        let temp_dir = std::env::temp_dir().join(format!("aegis_zip_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        // Create a small zip archive in memory
        let mut zip_buf = Vec::new();
        {
            let mut writer = zip::ZipWriter::new(Cursor::new(&mut zip_buf));
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);
            writer.start_file("hello.txt", options).unwrap();
            writer.write_all(b"Hello from ZIP!").unwrap();
            writer.start_file("sub/file.txt", options).unwrap();
            writer.write_all(b"Sub content").unwrap();
            writer.finish().unwrap();
        }

        let extracted = extract_zip_safely(&zip_buf, &temp_dir, None).unwrap();
        assert_eq!(extracted.len(), 2);
        assert_eq!(
            std::fs::read_to_string(temp_dir.join("hello.txt")).unwrap(),
            "Hello from ZIP!"
        );
        assert_eq!(
            std::fs::read_to_string(temp_dir.join("sub/file.txt")).unwrap(),
            "Sub content"
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_extract_zip_prevents_zipslip() {
        let temp_dir = std::env::temp_dir().join(format!("aegis_zip_slip_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        let mut zip_buf = Vec::new();
        {
            let mut writer = zip::ZipWriter::new(Cursor::new(&mut zip_buf));
            let options = zip::write::SimpleFileOptions::default();
            writer.start_file("../evil.txt", options).unwrap();
            writer.write_all(b"Malicious!").unwrap();
            writer.finish().unwrap();
        }

        let err = extract_zip_safely(&zip_buf, &temp_dir, None);
        assert!(err.is_err());
        assert!(err
            .unwrap_err()
            .to_string()
            .contains("Malicious ZIP path detected"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
