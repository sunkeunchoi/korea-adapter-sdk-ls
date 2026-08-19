use std::path::{Path, PathBuf};

use tempfile::TempDir;

pub struct FingerprintFixture {
    temp: TempDir,
}

impl FingerprintFixture {
    pub fn new() -> Self {
        let temp = TempDir::new().expect("create fingerprint fixture");
        seed(temp.path());
        Self { temp }
    }

    pub fn root(&self) -> &Path {
        self.temp.path()
    }

    pub fn path(&self, relative: &str) -> PathBuf {
        self.root().join(relative)
    }

    pub fn append(&self, relative: &str, bytes: &[u8]) {
        use std::io::Write as _;

        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(self.path(relative))
            .expect("open fixture input for append");
        file.write_all(bytes).expect("mutate fixture input");
    }
}

fn seed(root: &Path) {
    for directory in [
        "adapters/nautilus/lab/src/runner",
        "adapters/nautilus/src",
        "adapters/nautilus/nautilus-ls-calendar/src",
        "crates/ls-sdk/src",
        "crates/ls-core/src",
        "crates/ls-sdk-test-support/src",
        "metadata/constraints",
        "target/debug",
    ] {
        std::fs::create_dir_all(root.join(directory)).expect("create fixture directory");
    }

    for (relative, body) in [
        ("Cargo.toml", "[workspace]\n"),
        ("Cargo.lock", "root lock negative control\n"),
        ("adapters/nautilus/Cargo.toml", "[workspace]\n"),
        ("adapters/nautilus/Cargo.lock", "adapter lock\n"),
        (
            "adapters/nautilus/rust-toolchain.toml",
            "[toolchain]\nchannel = \"1.96\"\n",
        ),
        (
            "adapters/nautilus/lab/Cargo.toml",
            "[package]\nname = \"fixture-lab\"\n",
        ),
        ("adapters/nautilus/lab/build.rs", "fn main() {}\n"),
        (
            "adapters/nautilus/lab/fingerprint_core.rs",
            "fn shared() {}\n",
        ),
        ("adapters/nautilus/lab/src/lib.rs", "pub fn lab() {}\n"),
        (
            "adapters/nautilus/lab/src/runner/research.rs",
            "pub fn research() {}\n",
        ),
        ("adapters/nautilus/src/lib.rs", "pub fn adapter() {}\n"),
        (
            "adapters/nautilus/nautilus-ls-calendar/Cargo.toml",
            "[package]\nname = \"calendar\"\n",
        ),
        (
            "adapters/nautilus/nautilus-ls-calendar/src/lib.rs",
            "pub fn calendar() {}\n",
        ),
        ("crates/ls-sdk/Cargo.toml", "[package]\nname = \"ls-sdk\"\n"),
        ("crates/ls-sdk/src/lib.rs", "pub fn sdk() {}\n"),
        (
            "crates/ls-core/Cargo.toml",
            "[package]\nname = \"ls-core\"\n",
        ),
        ("crates/ls-core/build.rs", "fn main() {}\n"),
        ("crates/ls-core/src/lib.rs", "pub fn core() {}\n"),
        (
            "crates/ls-sdk-test-support/src/lib.rs",
            "pub fn support() {}\n",
        ),
        ("metadata/error-catalog.yaml", "errors: {}\n"),
        ("metadata/constraints/t1101.yaml", "tr: t1101\n"),
        ("target/debug/generated.rs", "generated negative control\n"),
    ] {
        let path = root.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create fixture file parent");
        }
        std::fs::write(path, body).expect("write fixture file");
    }
}
