// Shared declared-input inventory and hashing core for the lab build fingerprint.
//
// This file is included verbatim by both `build.rs` and `src/fingerprint.rs`.
// It intentionally depends only on `std` plus `sha2` so build-time embedding,
// runtime recomputation, and Cargo rebuild watches cannot acquire separate input
// lists or different traversal behavior.

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum FingerprintInputKind {
    File,
    Tree,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FingerprintInput {
    logical_label: String,
    relative_path: std::path::PathBuf,
    kind: FingerprintInputKind,
}

impl FingerprintInput {
    pub fn file(label: impl Into<String>, relative_path: impl Into<std::path::PathBuf>) -> Self {
        Self {
            logical_label: label.into(),
            relative_path: relative_path.into(),
            kind: FingerprintInputKind::File,
        }
    }

    pub fn tree(label: impl Into<String>, relative_path: impl Into<std::path::PathBuf>) -> Self {
        Self {
            logical_label: label.into(),
            relative_path: relative_path.into(),
            kind: FingerprintInputKind::Tree,
        }
    }

    pub fn logical_label(&self) -> &str {
        &self.logical_label
    }

    pub fn relative_path(&self) -> &std::path::Path {
        &self.relative_path
    }

    pub fn kind(&self) -> FingerprintInputKind {
        self.kind
    }

    pub fn covers(&self, relative_path: &std::path::Path) -> bool {
        let Ok(declared) = normalize_relative(&self.relative_path) else {
            return false;
        };
        let Ok(candidate) = normalize_relative(relative_path) else {
            return false;
        };
        candidate == declared
            || (self.kind == FingerprintInputKind::Tree && candidate.starts_with(&declared))
    }
}

/// The complete repository-local build-input prerequisite certified by
/// `LAB_SRC_FINGERPRINT`: the source and manifest of every repository-local crate
/// the lab compiles, the metadata a build script embeds, and the workspace
/// manifest, lockfile, and pinned toolchain that decide how those crates resolve.
pub fn declared_inventory() -> Vec<FingerprintInput> {
    vec![
        FingerprintInput::tree("lab-source", "adapters/nautilus/lab/src"),
        FingerprintInput::file("lab-manifest", "adapters/nautilus/lab/Cargo.toml"),
        FingerprintInput::file("lab-build-script", "adapters/nautilus/lab/build.rs"),
        FingerprintInput::file(
            "lab-fingerprint-core",
            "adapters/nautilus/lab/fingerprint_core.rs",
        ),
        FingerprintInput::file("root-workspace-manifest", "Cargo.toml"),
        FingerprintInput::tree("ls-sdk-source", "crates/ls-sdk/src"),
        FingerprintInput::file("ls-sdk-manifest", "crates/ls-sdk/Cargo.toml"),
        FingerprintInput::tree("ls-core-source", "crates/ls-core/src"),
        FingerprintInput::file("ls-core-manifest", "crates/ls-core/Cargo.toml"),
        FingerprintInput::file("ls-core-build-script", "crates/ls-core/build.rs"),
        FingerprintInput::file("error-catalog", "metadata/error-catalog.yaml"),
        FingerprintInput::tree("constraint-metadata", "metadata/constraints"),
        FingerprintInput::tree("adapter-source", "adapters/nautilus/src"),
        FingerprintInput::tree("calendar-source", "adapters/nautilus/nautilus-ls-calendar/src"),
        FingerprintInput::file(
            "calendar-manifest",
            "adapters/nautilus/nautilus-ls-calendar/Cargo.toml",
        ),
        FingerprintInput::file("adapter-workspace-manifest", "adapters/nautilus/Cargo.toml"),
        FingerprintInput::file("adapter-workspace-lock", "adapters/nautilus/Cargo.lock"),
        FingerprintInput::file(
            "adapter-rust-toolchain",
            "adapters/nautilus/rust-toolchain.toml",
        ),
    ]
}

pub fn compute_declared_fingerprint(repo_root: &std::path::Path) -> std::io::Result<String> {
    compute_from_inventory(repo_root, &declared_inventory())
}

pub fn compute_from_inventory(
    repo_root: &std::path::Path,
    inventory: &[FingerprintInput],
) -> std::io::Result<String> {
    let mut inventory = validate_inventory(repo_root, inventory)?;
    inventory.sort_by(|left, right| {
        left.input
            .logical_label
            .cmp(&right.input.logical_label)
            .then_with(|| left.normalized.cmp(&right.normalized))
            .then_with(|| left.input.kind.cmp(&right.input.kind))
    });

    let mut hasher = Sha256::new();
    hash_frame(&mut hasher, b"ls-lab-declared-build-inputs-v2");
    hash_u64(&mut hasher, inventory.len() as u64);
    for validated in inventory {
        let logical_path = portable_relative(&validated.normalized)?;
        hash_frame(&mut hasher, b"input");
        hash_frame(&mut hasher, kind_tag(validated.input.kind));
        hash_frame(&mut hasher, validated.input.logical_label.as_bytes());
        hash_frame(&mut hasher, logical_path.as_bytes());

        match validated.input.kind {
            FingerprintInputKind::File => {
                hash_frame(&mut hasher, &std::fs::read(&validated.absolute)?);
            }
            FingerprintInputKind::Tree => hash_tree(&mut hasher, &validated.absolute)?,
        }
    }

    let digest = hasher.finalize();
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        write!(&mut hex, "{byte:02x}").expect("writing to a String cannot fail");
    }
    Ok(hex)
}

pub fn watch_paths_from_root(
    repo_root: &std::path::Path,
) -> std::io::Result<Vec<std::path::PathBuf>> {
    let inventory = declared_inventory();
    let validated = validate_inventory(repo_root, &inventory)?;
    let mut paths: Vec<_> = validated.into_iter().map(|entry| entry.absolute).collect();
    paths.sort();
    Ok(paths)
}

struct ValidatedInput<'a> {
    input: &'a FingerprintInput,
    normalized: std::path::PathBuf,
    absolute: std::path::PathBuf,
}

fn validate_inventory<'a>(
    repo_root: &std::path::Path,
    inventory: &'a [FingerprintInput],
) -> std::io::Result<Vec<ValidatedInput<'a>>> {
    let root_meta = std::fs::symlink_metadata(repo_root)?;
    if !root_meta.file_type().is_dir() || root_meta.file_type().is_symlink() {
        return Err(invalid_data(
            "fingerprint repository root must be a real directory",
        ));
    }

    let mut labels = std::collections::BTreeSet::new();
    let mut physical_paths = std::collections::BTreeSet::new();
    let mut validated = Vec::with_capacity(inventory.len());
    for input in inventory {
        if input.logical_label.is_empty() || !labels.insert(input.logical_label.clone()) {
            return Err(invalid_data(format!(
                "duplicate or empty fingerprint label {:?}",
                input.logical_label
            )));
        }
        let normalized = normalize_relative(&input.relative_path)?;
        if normalized.as_os_str().is_empty() || !physical_paths.insert(normalized.clone()) {
            return Err(invalid_data(format!(
                "duplicate or empty fingerprint path {}",
                input.relative_path.display()
            )));
        }
        let absolute = repo_root.join(&normalized);
        let metadata = std::fs::symlink_metadata(&absolute).map_err(|error| {
            std::io::Error::new(
                error.kind(),
                format!("reading declared input {}: {error}", normalized.display()),
            )
        })?;
        if metadata.file_type().is_symlink() {
            return Err(invalid_data(format!(
                "declared input is a symlink: {}",
                normalized.display()
            )));
        }
        let type_matches = match input.kind {
            FingerprintInputKind::File => metadata.file_type().is_file(),
            FingerprintInputKind::Tree => metadata.file_type().is_dir(),
        };
        if !type_matches {
            return Err(invalid_data(format!(
                "declared input has the wrong node type: {}",
                normalized.display()
            )));
        }
        validated.push(ValidatedInput {
            input,
            normalized,
            absolute,
        });
    }

    for (index, left) in validated.iter().enumerate() {
        for right in validated.iter().skip(index + 1) {
            let left_contains_right = left.input.kind == FingerprintInputKind::Tree
                && right.normalized.starts_with(&left.normalized);
            let right_contains_left = right.input.kind == FingerprintInputKind::Tree
                && left.normalized.starts_with(&right.normalized);
            if left_contains_right || right_contains_left {
                return Err(invalid_data(format!(
                    "overlapping fingerprint paths {} and {}",
                    left.normalized.display(),
                    right.normalized.display()
                )));
            }
        }
    }
    Ok(validated)
}

fn hash_tree(hasher: &mut Sha256, tree_root: &std::path::Path) -> std::io::Result<()> {
    let mut nodes = Vec::new();
    collect_tree_nodes(tree_root, tree_root, &mut nodes)?;
    nodes.sort_by(|left, right| {
        left.relative
            .cmp(&right.relative)
            .then(left.kind.cmp(&right.kind))
    });
    hash_u64(hasher, nodes.len() as u64);
    for node in nodes {
        hash_frame(hasher, kind_tag(node.kind));
        hash_frame(hasher, portable_relative(&node.relative)?.as_bytes());
        if node.kind == FingerprintInputKind::File {
            hash_frame(hasher, &std::fs::read(node.absolute)?);
        }
    }
    Ok(())
}

struct TreeNode {
    relative: std::path::PathBuf,
    absolute: std::path::PathBuf,
    kind: FingerprintInputKind,
}

fn collect_tree_nodes(
    tree_root: &std::path::Path,
    directory: &std::path::Path,
    nodes: &mut Vec<TreeNode>,
) -> std::io::Result<()> {
    let relative = directory
        .strip_prefix(tree_root)
        .map_err(|_| invalid_data("tree escaped root"))?;
    nodes.push(TreeNode {
        relative: relative.to_path_buf(),
        absolute: directory.to_path_buf(),
        kind: FingerprintInputKind::Tree,
    });

    let mut entries: Vec<_> = std::fs::read_dir(directory)?.collect::<Result<_, _>>()?;
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        let absolute = entry.path();
        let metadata = std::fs::symlink_metadata(&absolute)?;
        let file_type = metadata.file_type();
        if file_type.is_symlink() {
            return Err(invalid_data(format!(
                "fingerprint tree contains a symlink: {}",
                absolute.display()
            )));
        }
        if file_type.is_dir() {
            collect_tree_nodes(tree_root, &absolute, nodes)?;
        } else if file_type.is_file() {
            nodes.push(TreeNode {
                relative: absolute
                    .strip_prefix(tree_root)
                    .map_err(|_| invalid_data("tree entry escaped root"))?
                    .to_path_buf(),
                absolute,
                kind: FingerprintInputKind::File,
            });
        } else {
            return Err(invalid_data(format!(
                "fingerprint tree contains a special node: {}",
                absolute.display()
            )));
        }
    }
    Ok(())
}

fn normalize_relative(path: &std::path::Path) -> std::io::Result<std::path::PathBuf> {
    use std::path::Component;

    if path.is_absolute() {
        return Err(invalid_data(format!(
            "fingerprint path must be relative: {}",
            path.display()
        )));
    }
    let mut normalized = std::path::PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(part) => normalized.push(part),
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(invalid_data(format!(
                    "fingerprint path cannot escape its root: {}",
                    path.display()
                )))
            }
        }
    }
    Ok(normalized)
}

fn portable_relative(path: &std::path::Path) -> std::io::Result<String> {
    let mut parts = Vec::new();
    for component in path.components() {
        let std::path::Component::Normal(part) = component else {
            continue;
        };
        parts.push(part.to_str().ok_or_else(|| {
            invalid_data(format!("non-UTF-8 fingerprint path: {}", path.display()))
        })?);
    }
    Ok(parts.join("/"))
}

fn kind_tag(kind: FingerprintInputKind) -> &'static [u8] {
    match kind {
        FingerprintInputKind::File => b"file",
        FingerprintInputKind::Tree => b"tree",
    }
}

fn hash_frame(hasher: &mut Sha256, bytes: &[u8]) {
    hash_u64(hasher, bytes.len() as u64);
    hasher.update(bytes);
}

fn hash_u64(hasher: &mut Sha256, value: u64) {
    hasher.update(value.to_be_bytes());
}

fn invalid_data(message: impl Into<String>) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, message.into())
}
