//! Offline end-to-end tests for the compiled Shu binary.
//!
//! Each test creates a real bare Git remote and uses Git's URL-rewrite feature
//! to map a neutral GitHub identity onto that local remote. This exercises Shu's
//! normal clone path without network access or global Git configuration changes.

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

use serde_json::Value;
use tempfile::TempDir;

const IDENTITY: &str = "github.com/example-org/api";

#[test]
fn restores_a_catalog_and_exposes_the_repository_to_agents() {
    let fixture = Fixture::new();
    let catalog = fixture.write_catalog("library.toml");

    fixture
        .shu(["--catalog", catalog.to_str().unwrap(), "--yes", "restore"])
        .assert_success();

    let expected = fixture
        .root
        .join("github.com")
        .join("example-org")
        .join("api");
    assert!(
        expected.join(".git").exists(),
        "restore should create a working clone"
    );

    let ensured = fixture.shu([
        "--catalog",
        catalog.to_str().unwrap(),
        "ensure",
        "api",
        "--path-only",
    ]);
    ensured.assert_success();
    assert_eq!(
        normalize_path(String::from_utf8(ensured.stdout).unwrap().trim()),
        normalize_path(&expected.to_string_lossy())
    );

    let listed = fixture.shu(["--catalog", catalog.to_str().unwrap(), "--json", "list"]);
    listed.assert_success();
    let json: Value = serde_json::from_slice(&listed.stdout).unwrap();
    assert_eq!(json["schema_version"], 1);
    assert_eq!(json["repositories"][0]["identity"], IDENTITY);
    assert_eq!(json["repositories"][0]["observed_state"], "present");
}

#[test]
fn activates_a_local_catalog_source_and_refreshes_it_with_update() {
    let fixture = Fixture::new();
    let source = fixture.write_catalog("portable-library.toml");
    let active = fixture.temp.path().join("active.toml");

    fixture
        .shu([
            "--catalog",
            active.to_str().unwrap(),
            "--yes",
            "restore",
            source.to_str().unwrap(),
        ])
        .assert_success();
    assert!(
        active.exists(),
        "restore should save the selected catalog as active"
    );
    assert!(
        active.with_extension("origin.json").exists(),
        "restore should remember its source for update"
    );

    fixture
        .shu(["--catalog", active.to_str().unwrap(), "--yes", "update"])
        .assert_success();
    fixture
        .shu([
            "--catalog",
            active.to_str().unwrap(),
            "path",
            "example-org/api",
        ])
        .assert_success();
}

#[test]
fn doctor_validates_a_restored_local_setup_and_its_source() {
    let fixture = Fixture::new();
    let source = fixture.write_catalog("portable-library.toml");
    let active = fixture.temp.path().join("active.toml");

    fixture
        .shu([
            "--catalog",
            active.to_str().unwrap(),
            "--yes",
            "restore",
            source.to_str().unwrap(),
        ])
        .assert_success();

    let doctor = fixture.shu([
        "--catalog",
        active.to_str().unwrap(),
        "doctor",
        "--check-source",
    ]);
    doctor.assert_success();
    let report = String::from_utf8(doctor.stdout).unwrap();
    assert!(report.contains("✓ git:"));
    assert!(report.contains("✓ catalog:"));
    assert!(report.contains("✓ catalog source: reachable:"));

    let json = fixture.shu(["--catalog", active.to_str().unwrap(), "--json", "doctor"]);
    json.assert_success();
    let report: Value = serde_json::from_slice(&json.stdout).unwrap();
    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["checks"][0]["name"], "git");
}

#[test]
fn picks_a_local_repository_without_an_external_fuzzy_finder() {
    let fixture = Fixture::new();
    let catalog = fixture.write_catalog("library.toml");
    fixture
        .shu(["--catalog", catalog.to_str().unwrap(), "--yes", "restore"])
        .assert_success();

    let picked = fixture.shu([
        "--catalog",
        catalog.to_str().unwrap(),
        "pick",
        "--filter",
        "api",
        "--path-only",
    ]);
    picked.assert_success();
    let expected = fixture
        .root
        .join("github.com")
        .join("example-org")
        .join("api");
    assert_eq!(
        normalize_path(String::from_utf8(picked.stdout).unwrap().trim()),
        normalize_path(&expected.to_string_lossy())
    );
}

#[test]
fn emits_navigation_wrappers_for_supported_shells() {
    for shell in ["bash", "zsh", "fish", "powershell", "nushell", "posix"] {
        let output = Command::new(env!("CARGO_BIN_EXE_shu"))
            .args(["shell", "init", shell])
            .output()
            .unwrap();
        output.assert_success();
        assert!(
            String::from_utf8(output.stdout)
                .unwrap()
                .contains("pick --path-only")
        );
    }
}

struct Fixture {
    temp: TempDir,
    root: PathBuf,
    rewrite_prefix: String,
}

impl Fixture {
    fn new() -> Self {
        let temp = TempDir::new().unwrap();
        let remotes = temp.path().join("remotes");
        let remote = remotes.join("api.git");
        let seed = temp.path().join("seed");

        run_git(temp.path(), ["init", "--bare"], Some(&remote));
        run_git(temp.path(), ["init"], Some(&seed));
        fs::write(seed.join("README.md"), "fixture\n").unwrap();
        run_git(&seed, ["add", "README.md"], None);
        run_git(
            &seed,
            [
                "-c",
                "user.name=Shu Test",
                "-c",
                "user.email=shu@example.invalid",
                "commit",
                "-m",
                "fixture",
            ],
            None,
        );
        run_git(&seed, ["branch", "-M", "main"], None);
        run_git(&seed, ["remote", "add", "origin"], Some(&remote));
        run_git(&seed, ["push", "-u", "origin", "main"], None);
        run_git(&remote, ["symbolic-ref", "HEAD", "refs/heads/main"], None);

        let rewrite_prefix = format!("file://{}/", remotes.to_string_lossy().replace('\\', "/"));
        Self {
            root: temp.path().join("library"),
            temp,
            rewrite_prefix,
        }
    }

    fn write_catalog(&self, name: &str) -> PathBuf {
        let path = self.temp.path().join(name);
        let root = self.root.to_string_lossy().replace('\\', "/");
        fs::write(&path, format!("version = 1\nroot = \"{root}\"\n\n[[repos]]\nsource = \"{IDENTITY}\"\nstate = \"active\"\ntags = [\"integration\"]\n")).unwrap();
        path
    }

    fn shu<const N: usize>(&self, args: [&str; N]) -> Output {
        let key = format!("url.{}.insteadOf", self.rewrite_prefix);
        Command::new(env!("CARGO_BIN_EXE_shu"))
            .args(args)
            .env("GIT_CONFIG_COUNT", "1")
            .env("GIT_CONFIG_KEY_0", key)
            .env("GIT_CONFIG_VALUE_0", "https://github.com/example-org/")
            .output()
            .unwrap()
    }
}

trait OutputExt {
    fn assert_success(&self);
}

impl OutputExt for Output {
    fn assert_success(&self) {
        assert!(
            self.status.success(),
            "command failed\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&self.stdout),
            String::from_utf8_lossy(&self.stderr)
        );
    }
}

fn run_git<const N: usize>(working_dir: &Path, args: [&str; N], path_argument: Option<&Path>) {
    let mut command = Command::new("git");
    command.current_dir(working_dir).args(args);
    if let Some(path) = path_argument {
        command.arg(path);
    }
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "git failed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn normalize_path(path: &str) -> String {
    path.replace('\\', "/")
}
