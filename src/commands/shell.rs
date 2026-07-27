//! Persistent shell integration for repository navigation.

use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, anyhow};

use crate::cli::{Shell, ShellCommands, ShellInitArgs};

const BEGIN_MARKER: &str = "# >>> shu shell integration >>>";
const END_MARKER: &str = "# <<< shu shell integration <<<";

/// Install or print a navigation wrapper for the requested shell.
pub fn shell(command: &ShellCommands) -> Result<()> {
    match command {
        ShellCommands::Init(args) => init(args),
    }
}

/// Install the wrapper once, or print it for a caller that wants to manage the file itself.
fn init(args: &ShellInitArgs) -> Result<()> {
    let script = integration(args.shell);
    if args.print {
        print!("{script}");
        return Ok(());
    }

    let path = match &args.path {
        Some(path) => path.clone(),
        None => default_profile(args.shell)?,
    };
    install(&path, script)?;
    println!("Installed Shu navigation for {}.", shell_name(args.shell));
    println!("  Startup file: {}", path.display());
    println!(
        "  Open a new {} session, then run `shu` to pick a repository.",
        shell_name(args.shell)
    );
    println!("  This command cannot change the shell that is already running.");
    Ok(())
}

/// Return the shell-specific startup file that Shu manages by default.
fn default_profile(shell: Shell) -> Result<PathBuf> {
    let home = home_dir()?;
    match shell {
        Shell::Bash => Ok(home.join(".bashrc")),
        Shell::Zsh => Ok(env::var_os("ZDOTDIR")
            .map(PathBuf::from)
            .unwrap_or(home)
            .join(".zshrc")),
        Shell::Fish => Ok(config_home(&home)
            .join("fish")
            .join("conf.d")
            .join("shu.fish")),
        Shell::Power => powershell_profile(&home),
        Shell::Nushell => nushell_profile(&home),
        Shell::Posix => Ok(home.join(".profile")),
    }
}

/// Ask PowerShell for its real current-user profile, with a portable fallback.
fn powershell_profile(home: &Path) -> Result<PathBuf> {
    for executable in ["pwsh", "powershell"] {
        if let Ok(output) = Command::new(executable)
            .args(["-NoProfile", "-Command", "$PROFILE.CurrentUserCurrentHost"])
            .output()
            && output.status.success()
        {
            let value = String::from_utf8_lossy(&output.stdout).trim().to_owned();
            if !value.is_empty() {
                return Ok(PathBuf::from(value));
            }
        }
    }
    Ok(home
        .join("Documents")
        .join("PowerShell")
        .join("Microsoft.PowerShell_profile.ps1"))
}

/// Ask Nushell for its configuration path, with an XDG-compatible fallback.
fn nushell_profile(home: &Path) -> Result<PathBuf> {
    if let Ok(output) = Command::new("nu").args(["-c", "$nu.config-path"]).output()
        && output.status.success()
    {
        let value = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        if !value.is_empty() {
            return Ok(PathBuf::from(value));
        }
    }
    Ok(config_home(home).join("nushell").join("config.nu"))
}

/// Add or replace Shu's marked integration block without touching other configuration.
fn install(path: &Path, script: &str) -> Result<()> {
    let previous = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => {
            return Err(error).with_context(|| format!("could not read {}", path.display()));
        }
    };
    let cleaned = remove_existing_block(&previous)?;
    let mut content = cleaned.trim_end().to_owned();
    if !content.is_empty() {
        content.push_str("\n\n");
    }
    content.push_str(script);

    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("startup file has no parent directory: {}", path.display()))?;
    fs::create_dir_all(parent)
        .with_context(|| format!("could not create startup directory {}", parent.display()))?;
    fs::write(path, content).with_context(|| format!("could not write {}", path.display()))
}

/// Remove the one block Shu owns while preserving every other line in a startup file.
fn remove_existing_block(content: &str) -> Result<String> {
    let Some(start) = content.find(BEGIN_MARKER) else {
        return Ok(content.to_owned());
    };
    let end_start = content[start..]
        .find(END_MARKER)
        .map(|offset| start + offset)
        .ok_or_else(|| anyhow!("found an incomplete Shu integration block; add `{END_MARKER}` or remove the block manually"))?;
    let end = end_start + END_MARKER.len();
    let mut result = String::with_capacity(content.len());
    result.push_str(&content[..start]);
    result.push_str(content[end..].trim_start_matches(['\r', '\n']));
    Ok(result)
}

/// Return the user's home directory from the platform directory provider.
fn home_dir() -> Result<PathBuf> {
    directories::BaseDirs::new()
        .map(|dirs| dirs.home_dir().to_path_buf())
        .ok_or_else(|| anyhow!("could not determine the home directory"))
}

/// Return the user configuration root while honoring the XDG override when present.
fn config_home(home: &Path) -> PathBuf {
    env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".config"))
}

/// Return a friendly name for setup output.
fn shell_name(shell: Shell) -> &'static str {
    match shell {
        Shell::Bash => "Bash",
        Shell::Zsh => "Zsh",
        Shell::Fish => "Fish",
        Shell::Power => "PowerShell",
        Shell::Nushell => "Nushell",
        Shell::Posix => "POSIX shell",
    }
}

/// Build the marked wrapper block for one shell.
fn integration(shell: Shell) -> &'static str {
    match shell {
        Shell::Bash | Shell::Zsh | Shell::Posix => POSIX_SCRIPT,
        Shell::Fish => FISH_SCRIPT,
        Shell::Power => POWERSHELL_SCRIPT,
        Shell::Nushell => NUSHELL_SCRIPT,
    }
}

const POSIX_SCRIPT: &str = r#"# >>> shu shell integration >>>
# Navigate to a Shu repository with bare `shu`; pass arguments to the binary.
shu() {
  if [ "$#" -eq 0 ]; then
    _shu_directory="$(command shu pick --path-only)"
    _shu_status=$?
    if [ "$_shu_status" -eq 0 ] && [ -n "$_shu_directory" ]; then
      cd -- "$_shu_directory"
    fi
    return "$_shu_status"
  fi
  command shu "$@"
}
# <<< shu shell integration <<<
"#;

const FISH_SCRIPT: &str = r#"# >>> shu shell integration >>>
# Navigate to a Shu repository with bare `shu`; pass arguments to the binary.
function shu --description 'Navigate Shu repositories or run Shu commands'
    if test (count $argv) -eq 0
        set -l _shu_directory (command shu pick --path-only)
        set -l _shu_status $status
        if test $_shu_status -eq 0; and test -n "$_shu_directory"
            cd -- $_shu_directory
        end
        return $_shu_status
    end
    command shu $argv
end
# <<< shu shell integration <<<
"#;

const POWERSHELL_SCRIPT: &str = r#"# >>> shu shell integration >>>
# Navigate to a Shu repository with bare `shu`; pass arguments to the binary.
function shu {
    param([Parameter(ValueFromRemainingArguments = $true)][string[]]$ShuArgs)
    $ShuBinary = (Get-Command shu -CommandType Application | Select-Object -First 1).Source
    if ($ShuArgs.Count -eq 0) {
        $ShuDirectory = & $ShuBinary pick --path-only
        if ($LASTEXITCODE -eq 0 -and $ShuDirectory) {
            Set-Location -LiteralPath $ShuDirectory
        }
        return
    }
    & $ShuBinary @ShuArgs
}
# <<< shu shell integration <<<
"#;

const NUSHELL_SCRIPT: &str = r#"# >>> shu shell integration >>>
# Navigate to a Shu repository with bare `shu`; pass arguments to the binary.
def --env shu [...shu_args] {
    if ($shu_args | is-empty) {
        let shu_directory = (^shu pick --path-only | str trim)
        if not ($shu_directory | is-empty) {
            cd $shu_directory
        }
    } else {
        ^shu ...$shu_args
    }
}
# <<< shu shell integration <<<
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn every_shell_script_invokes_the_path_only_picker() {
        for shell in [
            Shell::Bash,
            Shell::Zsh,
            Shell::Fish,
            Shell::Power,
            Shell::Nushell,
            Shell::Posix,
        ] {
            assert!(integration(shell).contains("pick --path-only"));
            assert!(integration(shell).contains(BEGIN_MARKER));
        }
    }

    #[test]
    fn installing_twice_replaces_only_shus_block() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("profile");
        fs::write(&path, "before\n# >>> shu shell integration >>>\nold\n# <<< shu shell integration <<<\nafter\n").unwrap();

        install(&path, POSIX_SCRIPT).unwrap();
        install(&path, POSIX_SCRIPT).unwrap();
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.starts_with("before"));
        assert!(content.contains("after"));
        assert_eq!(content.matches(BEGIN_MARKER).count(), 1);
        assert!(content.contains("pick --path-only"));
    }
}
