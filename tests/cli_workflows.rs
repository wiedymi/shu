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
fn restore_continues_after_an_inaccessible_repository_and_explains_the_failure() {
    let fixture = Fixture::new();
    let catalog = fixture.write_catalog_with_inaccessible_repository("library.toml");

    let restored = fixture.shu(["--catalog", catalog.to_str().unwrap(), "--yes", "restore"]);
    assert!(
        !restored.status.success(),
        "restore should report the inaccessible repository"
    );
    assert!(
        fixture
            .root
            .join("github.com")
            .join("example-org")
            .join("api")
            .join(".git")
            .exists(),
        "restore should continue with repositories that are accessible"
    );
    let stderr = String::from_utf8(restored.stderr).unwrap();
    assert!(stderr.contains("Check your internet connection and Git access"));
    assert!(stderr.contains("repositories that were accessible were restored"));
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
    for shell in [
        "bash",
        "zsh",
        "fish",
        "powershell",
        "pwsh",
        "nushell",
        "posix",
    ] {
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

#[test]
fn initializes_a_missing_catalog_without_blocking_everyday_commands() {
    let fixture = Fixture::new();
    let catalog = fixture.temp.path().join("new-library.toml");

    let status = fixture.shu(["--catalog", catalog.to_str().unwrap(), "status"]);
    status.assert_success();
    assert!(catalog.exists(), "status should create the missing catalog");
    let report = String::from_utf8(status.stdout).unwrap();
    assert!(report.contains("No repositories are catalogued yet."));
    assert!(report.contains("shu add ."));

    let picked = fixture.shu([
        "--catalog",
        catalog.to_str().unwrap(),
        "pick",
        "--path-only",
    ]);
    picked.assert_success();
    assert!(picked.stdout.is_empty());
}

#[test]
fn explains_missing_repositories_and_edits_metadata_from_a_local_clone() {
    let fixture = Fixture::new();
    let catalog = fixture.write_catalog("library.toml");

    let duplicate = fixture.shu([
        "--catalog",
        catalog.to_str().unwrap(),
        "add",
        fixture.seed.to_str().unwrap(),
    ]);
    assert!(!duplicate.status.success());
    let duplicate_error = String::from_utf8(duplicate.stderr).unwrap();
    assert!(duplicate_error.contains("already in the catalog"));
    assert!(duplicate_error.contains("shu edit api --state <state> --note <text>"));

    let mistaken_update = fixture.shu([
        "--catalog",
        catalog.to_str().unwrap(),
        "update",
        fixture.seed.to_str().unwrap(),
        "--state",
        "active",
    ]);
    assert!(!mistaken_update.status.success());
    assert!(
        String::from_utf8(mistaken_update.stderr)
            .unwrap()
            .contains("shu edit")
    );

    fixture
        .shu([
            "--catalog",
            catalog.to_str().unwrap(),
            "edit",
            fixture.seed.to_str().unwrap(),
            "--state",
            "parked",
            "--note",
            "Keep the first working prototype.",
        ])
        .assert_success();

    let status = fixture.shu(["--catalog", catalog.to_str().unwrap(), "status"]);
    status.assert_success();
    let report = String::from_utf8(status.stdout).unwrap();
    assert!(report.contains("PARKED"));
    assert!(report.contains("missing"));
    assert!(report.contains("Expected:"));
    assert!(report.contains("shu ensure api"));
    assert!(report.contains("Keep the first working prototype."));

    fixture
        .shu([
            "--catalog",
            catalog.to_str().unwrap(),
            "edit",
            "api",
            "--clear-note",
        ])
        .assert_success();
}

#[test]
fn migrates_a_clean_repository_into_shus_canonical_layout() {
    let fixture = Fixture::new();
    let catalog = fixture.write_empty_catalog("library.toml");
    let expected = fixture
        .root
        .join("github.com")
        .join("example-org")
        .join("api");

    let preview = fixture.shu([
        "--catalog",
        catalog.to_str().unwrap(),
        "add",
        fixture.seed.to_str().unwrap(),
        "--migrate",
        "--dry-run",
    ]);
    preview.assert_success();
    assert!(
        fixture.seed.exists(),
        "dry runs must not move the repository"
    );
    assert!(!expected.exists(), "dry runs must not create a destination");
    assert!(
        String::from_utf8(preview.stdout)
            .unwrap()
            .contains("Dry run: no files or catalog entries were changed.")
    );

    let migrated = fixture.shu([
        "--catalog",
        catalog.to_str().unwrap(),
        "--yes",
        "add",
        fixture.seed.to_str().unwrap(),
        "--migrate",
    ]);
    migrated.assert_success();
    assert!(
        !fixture.seed.exists(),
        "migration should move the source directory"
    );
    assert!(expected.join(".git").exists());
    let output = String::from_utf8(migrated.stdout).unwrap();
    assert!(output.contains("Working tree is clean"));
    assert!(output.contains("Moved repository"));
    assert!(output.contains("Added to catalog"));

    let status = fixture.shu(["--catalog", catalog.to_str().unwrap(), "status"]);
    status.assert_success();
    assert!(
        String::from_utf8(status.stdout)
            .unwrap()
            .contains("present")
    );
}

#[test]
fn migrates_an_already_catalogued_repository_without_overwriting_metadata() {
    let fixture = Fixture::new();
    let catalog = fixture.write_catalog("library.toml");
    let expected = fixture
        .root
        .join("github.com")
        .join("example-org")
        .join("api");

    let migrated = fixture.shu([
        "--catalog",
        catalog.to_str().unwrap(),
        "--yes",
        "add",
        fixture.seed.to_str().unwrap(),
        "--migrate",
    ]);
    migrated.assert_success();
    assert!(!fixture.seed.exists());
    assert!(expected.join(".git").exists());
    assert!(
        String::from_utf8(migrated.stdout)
            .unwrap()
            .contains("Preserved existing catalog metadata")
    );
}

#[test]
fn refuses_to_migrate_a_repository_with_uncommitted_changes() {
    let fixture = Fixture::new();
    let catalog = fixture.write_empty_catalog("library.toml");
    fs::write(fixture.seed.join("uncommitted.txt"), "do not move\n").unwrap();

    let migrated = fixture.shu([
        "--catalog",
        catalog.to_str().unwrap(),
        "--yes",
        "add",
        fixture.seed.to_str().unwrap(),
        "--migrate",
    ]);
    assert!(!migrated.status.success());
    assert!(fixture.seed.exists(), "dirty sources must stay in place");
    assert!(
        String::from_utf8(migrated.stderr)
            .unwrap()
            .contains("working tree has staged, unstaged, or untracked changes")
    );
}

#[test]
fn refuses_to_migrate_a_repository_with_linked_worktrees() {
    let fixture = Fixture::new();
    let catalog = fixture.write_empty_catalog("library.toml");
    let linked_worktree = fixture.temp.path().join("linked-worktree");
    run_git(
        &fixture.seed,
        ["worktree", "add", "--detach"],
        Some(&linked_worktree),
    );

    let migrated = fixture.shu([
        "--catalog",
        catalog.to_str().unwrap(),
        "--yes",
        "add",
        fixture.seed.to_str().unwrap(),
        "--migrate",
    ]);

    assert!(!migrated.status.success());
    assert!(
        fixture.seed.exists(),
        "linked working trees must stay in place"
    );
    assert!(
        String::from_utf8(migrated.stderr)
            .unwrap()
            .contains("repository has linked Git worktrees")
    );
}

#[cfg(windows)]
#[test]
fn powershell_setup_command_is_evaluable_and_forwards_to_the_binary() {
    let binary = PathBuf::from(env!("CARGO_BIN_EXE_shu"));
    let binary_directory = binary.parent().unwrap();
    let quote = |path: &Path| path.to_string_lossy().replace('\'', "''");
    let command = format!(
        "$env:Path = '{};' + $env:Path; Invoke-Expression ((& '{}' shell init pwsh) -join [Environment]::NewLine); shu --version",
        quote(binary_directory),
        quote(&binary),
    );
    let output = Command::new("pwsh")
        .args(["-NoProfile", "-Command", &command])
        .output()
        .unwrap();
    output.assert_success();
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .starts_with("shu ")
    );
}

struct Fixture {
    temp: TempDir,
    root: PathBuf,
    seed: PathBuf,
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
        run_git(
            &seed,
            [
                "remote",
                "set-url",
                "origin",
                "git@github.com:example-org/api.git",
            ],
            None,
        );

        let rewrite_prefix = format!("file://{}/", remotes.to_string_lossy().replace('\\', "/"));
        Self {
            root: temp.path().join("library"),
            temp,
            seed,
            rewrite_prefix,
        }
    }

    fn write_catalog(&self, name: &str) -> PathBuf {
        let path = self.temp.path().join(name);
        let root = self.root.to_string_lossy().replace('\\', "/");
        fs::write(&path, format!("version = 1\nroot = \"{root}\"\n\n[[repos]]\nsource = \"{IDENTITY}\"\nstate = \"active\"\ntags = [\"integration\"]\n")).unwrap();
        path
    }

    fn write_empty_catalog(&self, name: &str) -> PathBuf {
        let path = self.temp.path().join(name);
        let root = self.root.to_string_lossy().replace('\\', "/");
        fs::write(&path, format!("version = 1\nroot = \"{root}\"\n")).unwrap();
        path
    }

    fn write_catalog_with_inaccessible_repository(&self, name: &str) -> PathBuf {
        let path = self.write_catalog(name);
        let mut content = fs::read_to_string(&path).unwrap();
        content.push_str(
            "\n[[repos]]\nsource = \"github.com/example-org/unavailable\"\nstate = \"active\"\n",
        );
        fs::write(&path, content).unwrap();
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
