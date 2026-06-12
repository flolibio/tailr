# CLI Subcommand Refactoring Plan

## Current Problem

`tailr` uses boolean flags for operations that don't start the server. This creates 7 conditional branches in `main()`, 6 of which are early returns:

```
tailr --init              # flag, but doesn't start server
tailr --show-config       # flag, but doesn't start server
tailr --stop              # flag, but doesn't start server
tailr --status            # flag, but doesn't start server
tailr --systemd           # flag, but doesn't start server
tailr --launchd           # flag, but doesn't start server
tailr upgrade             # subcommand (already correct)
```

Issues:
1. Semantic confusion — `--stop` looks like a run mode, not a management action
2. `main()` is a chain of `if flag { return }` branches
3. All operation flags are crammed into `ServeArgs` alongside actual serve parameters
4. Hard to extend — each new operation adds another flag + branch

## Proposed Structure

```
tailr [FLAGS]                        # Start server (default, no subcommand)
tailr init                           # Initialize config file
tailr show                           # Show current configuration
tailr stop                           # Stop running daemon
tailr status                         # Show daemon status
tailr systemd                        # Generate systemd service file
tailr launchd                        # Generate launchd plist file
tailr upgrade [--check]              # Check/perform self-upgrade
```

### Server Flags (unchanged)

```
tailr -l /var/log/app -b :8080 -d
tailr --config /path/to/config.toml
tailr --pid-file /run/tailr.pid --log-file /var/log/tailr.log
```

### Subcommand Details

| Subcommand | Flags | Description |
|------------|-------|-------------|
| `init` | (none) | Print default config template to stdout |
| `show` | `--config <path>` | Show config file path and resolved values |
| `stop` | `--pid-file <path>` | Stop running daemon |
| `status` | `--pid-file <path>` | Show daemon running status |
| `systemd` | `-l <logs>`, `--user`, `--group` | Print systemd service file |
| `launchd` | `-l <logs>` | Print launchd plist file |
| `upgrade` | `-c`, `--check` | Check or perform self-upgrade |

## Clap Structure

```rust
#[derive(Parser)]
#[command(name = "tailr", version, about = "Log tail and search server")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    #[command(flatten)]
    serve: ServeArgs,
}

#[derive(Args)]
struct ServeArgs {
    #[arg(long)]
    config: Option<PathBuf>,

    #[arg(short, long, num_args = 1..)]
    log: Vec<PathBuf>,

    #[arg(short, long)]
    bind: Option<String>,

    #[arg(long, short)]
    daemon: bool,

    #[arg(long)]
    pid_file: Option<PathBuf>,

    #[arg(long)]
    log_file: Option<PathBuf>,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize config file and print to stdout
    Init,

    /// Show config file path and current configuration
    Show {
        #[arg(long)]
        config: Option<PathBuf>,
    },

    /// Stop running daemon
    Stop {
        #[arg(long)]
        pid_file: Option<PathBuf>,
    },

    /// Show daemon status
    Status {
        #[arg(long)]
        pid_file: Option<PathBuf>,
    },

    /// Generate systemd service file
    Systemd {
        #[arg(short, long, num_args = 1..)]
        log: Vec<PathBuf>,

        #[arg(long, default_value_t = whoami())]
        user: String,

        #[arg(long)]
        group: Option<String>,
    },

    /// Generate launchd plist file (macOS)
    Launchd {
        #[arg(short, long, num_args = 1..)]
        log: Vec<PathBuf>,
    },

    /// Check for updates and upgrade tailr
    Upgrade {
        #[arg(long, short = 'c')]
        check: bool,
    },
}
```

## main() Flow (After Refactoring)

```rust
fn main() {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Init) => {
            print!("{}", config::default_config_template());
        }
        Some(Commands::Show { config }) => {
            show_config(config);
        }
        Some(Commands::Stop { pid_file }) => {
            stop_daemon(pid_file);
        }
        Some(Commands::Status { pid_file }) => {
            show_status(pid_file);
        }
        Some(Commands::Systemd { log, user, group }) => {
            print_systemd(log, user, group);
        }
        Some(Commands::Launchd { log }) => {
            print_launchd(log);
        }
        Some(Commands::Upgrade { check }) => {
            run_upgrade(check);
        }
        None => {
            run_server(cli.serve);
        }
    }
}
```

No more early returns. Each branch is self-contained.

## Backward Compatibility

This is a **breaking change** for the CLI interface:

| Before | After |
|--------|-------|
| `tailr --init` | `tailr init` |
| `tailr --show-config` | `tailr show` |
| `tailr --stop` | `tailr stop` |
| `tailr --status` | `tailr status` |
| `tailr --systemd` | `tailr systemd` |
| `tailr --launchd` | `tailr launchd` |

Server flags are unchanged: `-l`, `-b`, `-d`, `--config`, `--pid-file`, `--log-file`.

**Version**: This should be released as **v0.3.0** (MINOR bump, since we're pre-1.0 and adding new functionality is acceptable even with breaking CLI changes during 0.x).

## Implementation Steps

1. **Refactor `Commands` enum** — Move `Init`, `Show`, `Stop`, `Status`, `Systemd`, `Launchd` from `ServeArgs` flags to subcommands
2. **Clean `ServeArgs`** — Remove `init`, `show_config`, `stop`, `status`, `systemd`, `launchd` fields
3. **Rewrite `main()`** — Replace if-chain with match on `cli.command`
4. **Add per-subcommand flags** — Each subcommand takes only the flags it needs
5. **Update `docs/release-guide.md`** and `AGENTS.md`** — Reflect new CLI interface
6. **Update `README.md`** — Update CLI usage examples
7. **Test** — Verify all commands work: `tailr init`, `tailr show`, `tailr stop`, `tailr status`, `tailr systemd`, `tailr launchd`, `tailr upgrade`, `tailr -l /var/log`

## File Changes

| File | Change |
|------|--------|
| `src/main.rs` | Major refactor — new subcommand enum, clean ServeArgs, match-based main |
| `AGENTS.md` | Update CLI section |
| `README.md` | Update CLI usage section |
| `docs/release-guide.md` | Update if CLI examples exist |
