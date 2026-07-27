//! Shell integration that lets bare `shu` select and enter a repository.

use anyhow::Result;

use crate::cli::{Shell, ShellCommands};

/// Print an integration wrapper for the requested shell syntax.
pub fn shell(command: &ShellCommands) -> Result<()> {
    match command {
        ShellCommands::Init(args) => print!("{}", init_script(args.shell)),
    }
    Ok(())
}

/// Return shell code that delegates normal commands to the binary and intercepts bare `shu`.
fn init_script(shell: Shell) -> &'static str {
    match shell {
        Shell::Bash | Shell::Zsh | Shell::Posix => POSIX_SCRIPT,
        Shell::Fish => FISH_SCRIPT,
        Shell::Power => POWERSHELL_SCRIPT,
        Shell::Nushell => NUSHELL_SCRIPT,
    }
}

const POSIX_SCRIPT: &str = r#"# Shu navigation integration. Add to your shell startup file.
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
"#;

const FISH_SCRIPT: &str = r#"# Shu navigation integration. Add to ~/.config/fish/conf.d/shu.fish.
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
"#;

const POWERSHELL_SCRIPT: &str = r#"# Shu navigation integration. Add to $PROFILE.
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
"#;

const NUSHELL_SCRIPT: &str = r#"# Shu navigation integration. Add to your Nushell config.nu.
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
"#;

#[cfg(test)]
mod tests {
    use super::*;

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
            assert!(init_script(shell).contains("pick --path-only"));
        }
    }
}
