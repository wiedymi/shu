#!/usr/bin/env sh
# A runnable, read-only preview of Shu's proposed terminal design language.
# It does not call Shu, read configuration, or make network requests.

set -eu

color=1
animate=1

usage() {
    printf '%s\n' "Usage: scripts/design-proposal.sh [--no-color] [--static]"
    printf '%s\n' "  --no-color  Render without terminal color."
    printf '%s\n' "  --static    Show representative progress frames without animation."
}

for argument in "$@"; do
    case "$argument" in
        --no-color) color=0 ;;
        --static) animate=0 ;;
        --help|-h) usage; exit 0 ;;
        *) printf 'Unknown option: %s\n' "$argument" >&2; usage >&2; exit 2 ;;
    esac
done

if ! test -t 1; then
    color=0
    animate=0
fi

if test "${TERM:-dumb}" = dumb; then
    color=0
fi

style() {
    if test "$color" -eq 1; then
        case "$1" in
            sgr0) printf '\033[0m' ;;
            bold) printf '\033[1m' ;;
            dim) printf '\033[2m' ;;
            setaf) printf '\033[%sm' "$2" ;;
            setab) printf '\033[%sm' "$2" ;;
        esac
    fi
}

reset() { style sgr0; }
muted() { style dim; }
accent() { style setaf 36; style bold; }
good() { style setaf 32; style bold; }
warn() { style setaf 33; style bold; }
bad() { style setaf 31; style bold; }
title() { style bold; }

# A single-cell input anchor. It is the only colored surface in the picker;
# the solid block keeps no-color output legible.
input_cell() {
    if test "$color" -eq 1; then
        style setab 46; printf ' '; reset
    else
        printf '█'
    fi
}

line() {
    printf '%s\n' '────────────────────────────────────────────────────────────────────────'
}

heading() {
    printf '\n'
    accent; printf 'shu'; reset; muted; printf ' / '; reset; title; printf '%s\n' "$1"; reset
    line
}

label() {
    muted; printf '%-12s' "$1"; reset
}

heading 'Terminal design proposal'
printf '%s\n' 'A quiet, information-dense interface for people, while scripts retain exact output.'
printf '%s\n' 'This preview is illustrative only: it performs no Shu operation and no network activity.'

heading '1. Design contract'
printf '%s\n' '  • One visual grammar: action → live work → durable result → next useful command.'
printf '%s\n' '  • Meaning comes before decoration: labels, verbs, paths, and states are always textual.'
printf '%s\n' '  • Styling is additive and only used on an interactive terminal; no style carries meaning alone.'
printf '%s\n' '  • stdout remains clean for values and --json. Human narration and live progress use stderr.'
printf '%s\n' '  • Progress is honest: indeterminate work says what is happening; byte work shows bytes when known.'
printf '%s\n' '  • No permanent success noise. A completed operation gets one compact summary.'
printf '%s\n' '  • Picking preserves terminal context: a compact, bottom-aligned filter—not a separate full-screen app.'

heading '2. Shared primitives'
printf '  '; good; printf '✓'; reset; printf '  completed or already true\n'
printf '  '; accent; printf '→'; reset; printf '  starting a meaningful step\n'
printf '  '; accent; printf '…'; reset; printf '  work in progress when total work is unknown\n'
printf '  '; warn; printf '!'; reset; printf '  attention needed; the operation can continue or explains the choice\n'
printf '  '; bad; printf '×'; reset; printf '  operation failed; show cause and the immediate recovery command\n'
printf '\n'
printf '  '; label 'result'; printf 'github.com/wiedymi/shu\n'
printf '  '; label 'location'; printf '/Users/wiedy/Code/github.com/wiedymi/shu\n'
printf '  '; label 'next'; printf 'shu status\n'

heading '3. Operations: truthful work feedback'
printf '%s\n' 'TTY-only work feedback is quiet and factual: a named spinner before a size is known, then real received bytes.'
printf '\n'
muted; printf 'Installing '; reset; title; printf 'shu'; reset; muted; printf '  version '; reset; printf '0.1.16\n'

if test "$animate" -eq 1; then
    for frame in '◐' '◓' '◑' '◒'; do
        printf '\r  '
        accent; printf '%s ' "$frame"; reset
        printf 'Finding release asset'
        sleep 0.12
    done
    printf '\r  '
    good; printf '✓ '; reset
    printf 'Found shu-darwin-arm64.tar.gz                              \n'
else
    printf '  '; accent; printf '… '; reset; printf 'Finding release asset\n'
    printf '  '; good; printf '✓ '; reset; printf 'Found shu-darwin-arm64.tar.gz\n'
fi
printf '  '; accent; printf '→ '; reset; printf 'Downloading shu-darwin-arm64.tar.gz\n'
printf '  '; accent; printf '[███████████······] '; reset; printf '4.2 MiB / 6.8 MiB  62%%\n'
printf '  '; good; printf '✓ '; reset; printf 'Installed shu 0.1.16\n'
printf '  '; label 'location'; printf '~/.local/bin/shu\n'
printf '  '; label 'next'; printf 'shu --version\n'

heading '4. Fuzzy picker: focused selection'
printf '%s\n' 'A compact bottom prompt keeps terminal context visible. Repository identity leads; the local path confirms the choice.'
printf '\n'
accent; printf 'Pick a repository\n'; reset
printf '\n'
input_cell; printf ' '; muted; printf 'Search repositories\n'; reset
printf '\n'
accent; printf '› '; reset; title; printf 'wiedymi/shu'; reset
printf '  '; accent; printf '◆'; reset
printf '\n  '; muted; printf '~/Code/github.com/wiedymi/shu\n'; reset
printf '\n  '; title; printf 'wiedymi/api'; reset
printf '  '; accent; printf '⎇ '; reset; muted; printf 'feature/auth'; reset
printf '\n  '; muted; printf '~/Code/github.com/wiedymi/api\n'; reset
printf '\n  '; title; printf 'acme/api-docs'; reset
printf '  '; accent; printf '◇'; reset
printf '\n  '; muted; printf '~/Code/github.com/acme/api-docs\n'; reset
printf '\n'; accent; printf '◆'; reset; muted; printf ' primary   '; reset
accent; printf '◇'; reset; muted; printf ' checkout   '; reset
accent; printf '⎇'; reset; muted; printf ' worktree\n'; reset
printf '\n'; muted; printf '↑↓ navigate  ·  / filter  ·  enter open  ·  esc cancel\n'; reset
printf '\n'
muted; printf 'After /\n'; reset
printf '\n'
accent; printf 'Find a repository\n'; reset
printf '\n'
input_cell; printf ' api\n\n'
accent; printf '› '; reset; title; printf 'wiedymi/api'; reset
printf '  '; accent; printf '⎇ '; reset; muted; printf 'feature/auth'; reset
printf '\n  '; muted; printf '~/Code/github.com/wiedymi/api\n'; reset
printf '\n  '; title; printf 'acme/api'; reset
printf '  '; accent; printf '◇'; reset
printf '\n  '; muted; printf '~/Code/github.com/acme/api\n'; reset
printf '\n  '; title; printf 'acme/api-docs'; reset
printf '  '; accent; printf '◆'; reset
printf '\n  '; muted; printf '~/Code/github.com/acme/api-docs\n'; reset
printf '\n'; muted; printf '3 matches  ·  esc cancel\n'; reset

heading '5. Status, confirmations, and errors'
printf '%s\n' 'Status groups by outcome, confirmations name the consequence, and errors pair a clear cause with one recovery.'
printf '\n'
accent; printf 'shu'; reset; printf '  Library status'; muted; printf '                                      12 repositories'; reset
printf '\n\n'
good; printf '✓ '; reset; printf 'Ready'; muted; printf '  10'; reset
printf '\n  github.com/wiedymi/shu'; muted; printf '                    /Users/wiedy/Code/github.com/wiedymi/shu'; reset
printf '\n\n'
warn; printf '! '; reset; printf 'Needs restore'; muted; printf '  2'; reset
printf '\n  github.com/wiedymi/archive'; muted; printf '                expected at ~/Code/github.com/wiedymi/archive'; reset
printf '\n  '; label 'next'; printf 'shu ensure github.com/wiedymi/archive\n'
printf '\n'
warn; printf '! '; reset; printf 'Move repository into Shu library?\n'
printf '  '; label 'from'; printf '/tmp/shu\n'
printf '  '; label 'to'; printf '~/Code/github.com/wiedymi/shu\n'
printf '  '; label 'effect'; printf 'moves the clean clone; Git history and files are unchanged\n'
printf '  Continue? [y/N] '
printf '\n\n'
bad; printf '× '; reset; printf 'Could not clone github.com/wiedymi/shu\n'
printf '  '; label 'cause'; printf 'authentication to github.com was rejected\n'
printf '  '; label 'next'; printf 'gh auth login, then rerun shu ensure github.com/wiedymi/shu\n'

heading '6. Everyday command surfaces'
printf '%s\n' 'Commands that create, publish, update, or explain themselves use the same compact action → result grammar.'
printf '\n'
accent; printf 'New repository\n'; reset
muted; printf '  shu new github.com/wiedymi/notes --github --private\n'; reset
printf '  '; accent; printf '→ '; reset; printf 'Create local repository\n'
printf '  '; accent; printf '→ '; reset; printf 'Create private GitHub repository\n'
printf '  '; good; printf '✓ '; reset; printf 'Created wiedymi/notes\n'
printf '  '; label 'location'; printf '~/Code/github.com/wiedymi/notes\n'
printf '  '; label 'next'; printf 'cd ~/Code/github.com/wiedymi/notes\n'
printf '\n'
accent; printf 'Set up catalog sync\n'; reset
muted; printf '  shu sync init github.com/wiedymi/repository-library --github --private\n'; reset
printf '  '; accent; printf '→ '; reset; printf 'Create catalog checkout\n'
printf '  '; accent; printf '→ '; reset; printf 'Publish shu.toml to github.com/wiedymi/repository-library\n'
printf '  '; good; printf '✓ '; reset; printf 'Catalog sync is ready\n'
printf '  '; label 'checkout'; printf '~/Code/github.com/wiedymi/repository-library\n'
printf '  '; label 'next'; printf 'shu sync\n'
printf '\n'
accent; printf 'Sync catalog\n'; reset
muted; printf '  shu sync\n'; reset
printf '  '; accent; printf '… '; reset; printf 'Checking github.com/wiedymi/repository-library\n'
printf '  '; good; printf '✓ '; reset; printf 'Published shu.toml\n'
printf '  '; label 'remote'; printf 'github.com/wiedymi/repository-library\n'
printf '\n'
accent; printf 'Upgrade\n'; reset
muted; printf '  shu upgrade\n'; reset
printf '  '; label 'version'; printf '0.1.15 → 0.1.16\n'
printf '  '; accent; printf '… '; reset; printf 'Downloading shu-darwin-arm64.tar.gz\n'
printf '  '; accent; printf '[███████████······] '; reset; printf '4.2 MiB / 6.8 MiB  62%%\n'
printf '  '; good; printf '✓ '; reset; printf 'Updated shu to 0.1.16\n'
printf '\n'
accent; printf 'Version\n'; reset
muted; printf '  shu --version\n'; reset
printf '  shu 0.1.16\n'
printf '\n'
accent; printf 'Help\n'; reset
muted; printf '  shu --help\n'; reset
printf '  shu — declarative repository library\n\n'
printf '  Usage: shu [OPTIONS] [COMMAND]\n\n'
printf '  Everyday commands\n'
printf '    add <repository>      Record a repository and clone it when needed\n'
printf '    new <repository>      Create a local repository, optionally on GitHub\n'
printf '    pick                  Find and open a local repository\n'
printf '    status                See what is ready, missing, or needs attention\n'
printf '    sync                  Publish the active catalog\n'
printf '    restore <source>      Load a saved catalog and restore missing clones\n'
printf '    upgrade               Install the latest verified Shu release\n\n'
muted; printf '  Run '; reset; printf 'shu <command> --help'; muted; printf ' for options and examples.\n'; reset

heading '7. Implementation boundary'
printf '%s\n' 'Apply this in a dedicated UI layer, not as formatting scattered through command logic:'
printf '%s\n' '  • Output mode is explicit: Machine (JSON/path), Human (persistent lines), or Interactive (picker/progress).'
printf '%s\n' '  • A single Progress state enum covers Pending, Indeterminate, Sized { done, total }, Complete, and Failed.'
printf '%s\n' '  • Commands report operation facts; the UI layer owns stream selection, color, layout, and cleanup.'
printf '%s\n' '  • Progress receives bounded byte counts only from the download stream—never invented percentages or timings.'
printf '%s\n' '  • The picker writes only to stderr, leaves stdout as the selected path, and uses a bottom-aligned bounded region.'
printf '%s\n' '  • Picker rows pair a repository identity with its local path; the location role removes ambiguity without becoming stored metadata.'
printf '%s\n' '  • A derived location role is a closed set: primary checkout, separate checkout, or worktree (with branch when Git reports one).'

heading '8. Proposed rollout'
printf '%s\n' '  1. Establish output modes and shared renderer with snapshot tests for non-TTY output.'
printf '%s\n' '  2. Adopt it for restore, ensure, add, upgrade, sync, and scan—the operations that can take noticeable time.'
printf '%s\n' '  3. Replace the picker with a bounded, fzf-like renderer after terminal-size, resize, Unicode-width, and stdout-capture behavior are covered.'
printf '%s\n' '  4. Bring catalog, status, doctor, and errors onto the same result grammar; preserve --json exactly.'
printf '\n'
good; printf '✓ '; reset; printf 'End of proposal. Run with --static for deterministic review output.\n'
