# Paladen

Paladen is a lightweight CLI account and credential manager built in Rust. It simplifies managing multiple SSH accounts, panels, and servers, with built-in support for interactive connections, remote command execution, and SCP file transfers.

## Features

- **SQLite Backend**: Securely stores your account details locally.
- **SSH Key Support**: Automatically uses your default SSH keys or specified identity files.
- **Remote Commands**: Execute commands on remote servers directly from the CLI.
- **SCP Support**: Easy file transfers with smart path auto-completion for remote targets.
- **Import/Export Friendly**: Support for clean `stdout` output, allowing for database dumps and piping.
- **Interactive Menu**: User-friendly interactive mode for daily management.

## Installation

### Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) (cargo)
- `ssh` and `scp` installed on your system.
- `sshpass` (optional, for automated password-based login).

### Building from source

```bash
git clone https://github.com/AryAntara/paladen.git
cd paladen
cargo build --release
```

The binary will be available at `target/release/paladen`. You can alias it to `paladen` in your shell.

## Usage

### 1. Adding an Account
Run the interactive menu to add your first account:
```bash
paladen add
```

### 2. SSH Connection
Connect to a saved account by its ID:
```bash
paladen ssh 1
```
If you omit the ID, it will open an interactive picker.

### 3. Running Remote Commands
Run commands directly without entering a full shell:
```bash
paladen ssh 1 'df -h && uptime'
```

### 4. SCP File Transfers
Transfer files easily. Use a leading colon `:` to indicate a remote path; Paladen will automatically fill in the `username@host`.

**Upload:**
```bash
paladen scp 1 ./local-file.txt :/var/www/html/
```

**Download:**
```bash
paladen scp 1 :/home/user/backup.tar.gz ./backups/
```

### 5. Database Exports (Redirection)
Paladen sends informational messages to `stderr`, keeping `stdout` clean for redirection:
```bash
paladen ssh 1 'mysqldump -u root database_name' > local_dump.sql
```

## Configuration

Paladen stores its database at `~/.config/accounts/accounts.db` by default. You can specify a custom database path using the `--db` flag:

```bash
paladen --db /path/to/custom.db list
```

## License

This project is licensed under the MIT License - see the LICENSE file for details.
