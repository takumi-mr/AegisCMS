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
    /// Signature target ("manifest" or "package", defaults to "manifest")
    #[serde(default = "default_signature_target")]
    pub target: String,
}

fn default_signature_target() -> String {
    "manifest".to_string()
}

/// Computes the canonical package digest over files in `package_dir` (excluding signature.json).
/// Files are sorted by their normalized slash-separated relative paths.
pub fn compute_package_files_digest(package_dir: &Path) -> Result<Vec<u8>> {
    let mut files = Vec::new();
    collect_files_recursive(package_dir, package_dir, &mut files)?;
    files.sort();

    let mut hasher = Sha256::new();
    for (rel_path, file_path) in files {
        if rel_path == "signature.json" {
            continue;
        }
        let content = std::fs::read(&file_path).map_err(AegisError::Io)?;
        let file_hash = Sha256::digest(&content);
        hasher.update(rel_path.as_bytes());
        hasher.update(b":");
        hasher.update(file_hash);
        hasher.update(b"\n");
    }

    Ok(hasher.finalize().to_vec())
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
    manifest_bytes: &[u8],
    trusted_keys: Option<&[String]>,
    require_signature: bool,
) -> Result<Option<String>> {
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

    // Prepare target payload to verify
    let target_data = match sig.target.as_str() {
        "manifest" => manifest_bytes.to_vec(),
        "package" => compute_package_files_digest(package_dir)?,
        other => {
            return Err(AegisError::Validation(format!(
                "Unsupported signature target '{}'",
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

/// Signs manifest or package content using an Ed25519 key pair for tests/tooling.
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

        // 1. Enclosed name validation against ZipSlip
        let enclosed = match file.enclosed_name() {
            Some(path) => path.to_owned(),
            None => {
                return Err(AegisError::Validation(format!(
                    "Malicious ZIP path detected: {:?}",
                    file.name()
                )));
            }
        };

        // Extra check on path components
        for comp in enclosed.components() {
            match comp {
                Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                    return Err(AegisError::Validation(format!(
                        "Illegal path component in ZIP entry: {:?}",
                        file.name()
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
                    file.name(),
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
                        file.name()
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
    fn test_signature_roundtrip_manifest() {
        let rng = ring::rand::SystemRandom::new();
        let pkcs8 = signature::Ed25519KeyPair::generate_pkcs8(&rng).unwrap();
        let key_pair = signature::Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).unwrap();

        let manifest_data = br#"{"name": "test-plugin", "version": "1.0.0"}"#;
        let sig = create_test_signature(manifest_data, &key_pair, "manifest");

        let temp_dir = std::env::temp_dir().join(format!("aegis_sig_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        std::fs::write(
            temp_dir.join("signature.json"),
            serde_json::to_string_pretty(&sig).unwrap(),
        )
        .unwrap();

        // 1. Valid signature without trusted keys restriction
        let verified_pk = verify_package_signature(&temp_dir, manifest_data, None, true).unwrap();
        assert_eq!(verified_pk, Some(sig.public_key.clone()));

        // 2. Valid signature with matching trusted key
        let verified_pk = verify_package_signature(
            &temp_dir,
            manifest_data,
            Some(&[sig.public_key.clone()]),
            true,
        )
        .unwrap();
        assert_eq!(verified_pk, Some(sig.public_key.clone()));

        // 3. Untrusted key rejection
        let err = verify_package_signature(
            &temp_dir,
            manifest_data,
            Some(&["0000000000000000000000000000000000000000000000000000000000000000".to_string()]),
            true,
        );
        assert!(err.is_err());
        assert!(err
            .unwrap_err()
            .to_string()
            .contains("not in trusted public keys list"));

        // 4. Tampered manifest data rejection
        let err = verify_package_signature(&temp_dir, b"tampered content", None, true);
        assert!(err.is_err());
        assert!(err
            .unwrap_err()
            .to_string()
            .contains("digital signature verification failed"));

        let _ = std::fs::remove_dir_all(&temp_dir);
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
