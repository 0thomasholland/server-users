# Server Users Monitor

[![Crates.io](https://img.shields.io/crates/v/server_users.svg)](https://crates.io/crates/server_users)
[![Downloads](https://img.shields.io/crates/d/server_users.svg)](https://crates.io/crates/server_users)

A terminal-based SSH server monitoring tool built with Rust that displays real-time CPU and RAM usage per user on remote servers.

Motivation was to have an easy way to see who was using all the resources on a shared server!

## Installation

### Prerequisites

- Rust 1.70 or higher
- SSH access to the target server(s)

### Build from Source

```bash
git clone https://github.com/0thomasholland/server-users.git
cd server-users
cargo build --release
```

The compiled binary will be available at the releases on [github](https://github.com/0thomasholland/server-users/releases).

Download the appropriate binary for your platform and extract it, make executable, and run.

## Usage

### Interactive Mode

Simply run the program without arguments for an interactive configuration screen:

```bash
cargo run
# or if using the binary
./target/release/server_users
```

Navigate through the configuration fields using:

- `Tab` / `Shift+Tab` - Move between fields
- `↑` / `↓` - Move between fields
- `Space` - Toggle SSH key authentication
- `Enter` - Connect to server
- `q` / `Esc` - Quit
- `c` - Sort by CPU usage
- `r` - Sort by RAM usage

### Command Line Mode

Connect directly by providing arguments:

```bash
# Using password authentication
 -s hostname.com -u username -p password

# Using SSH key authentication
-s hostname.com -u username --use-key

# Using custom SSH key path
-s hostname.com -u username --use-key -k ~/.ssh/custom_key
```

```
Options:
  -s, --server <SERVER>      SSH server hostname or IP address
  -u, --user <USER>         SSH username
  -p, --password <PASSWORD>  SSH password (if not using SSH key)
  -k, --key <SSH_KEY>       Path to SSH private key (default: ~/.ssh/id_rsa)
      --use-key             Use SSH key authentication instead of password
  -h, --help                Print help
  -V, --version             Print version
```

## Security Considerations

- Passwords provided via command line arguments may be visible in process lists
- For production use, SSH key authentication is recommended
- Ensure proper file permissions on SSH keys (typically `chmod 600`)
- The tool requires SSH access with sufficient privileges to run `ps` and `free` commands
