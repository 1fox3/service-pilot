# Service Pilot

Service Pilot is a Tauri desktop app for discovering and controlling local macOS services from one lightweight UI.

It currently supports Docker containers, Homebrew services, and user-defined custom services.

## Features

- Discover Docker containers from `docker ps -a --format json`
- Discover Homebrew services from `brew services list --json`
- Discover custom services from `~/servicepilot/customservices/*.json`
- Start, stop, and restart services
- Update Homebrew services with `brew upgrade <service>`
- View Docker logs and Homebrew service info
- Auto-load service config when a service is selected
- Edit known Homebrew config files when a writable config path is discovered
- Show Docker config through read-only `docker inspect`
- Open discovered Homebrew install paths in Finder
- Load Homebrew version, stable version, and install path during startup and Refresh All
- Show the Homebrew update action only when the installed version differs from the stable version
- Run user-defined start, stop, restart, status, logs, and config commands for custom services
- Add custom command services from the UI

## Service Providers

### Docker

Docker services are local containers discovered with:

```bash
docker ps -a --format json
```

Service actions use:

```bash
docker start <container>
docker stop <container>
docker restart <container>
docker logs --tail 160 <container>
docker inspect <container>
```

Docker config is read-only and comes from `docker inspect`.

### Homebrew

Homebrew services are discovered with:

```bash
brew services list --json
```

Service actions use:

```bash
brew services start <service>
brew services stop <service>
brew services restart <service>
brew services info <service>
brew upgrade <service>
```

Homebrew details are loaded during startup and Refresh All:

```bash
brew info --json=v2 <service>
brew --prefix <service>
```

When the installed version differs from the stable version, Service Pilot shows the version as `current -> stable` and displays the update button. If both versions match, the update button is hidden.

Known editable config paths include common Redis, Nginx, Prometheus, Grafana, and MySQL config locations under the Homebrew prefix.

### Custom Services

Custom services are defined as one JSON file per service under:

```text
~/servicepilot/customservices/
```

Example:

```json
[
  {
    "id": "python-worker",
    "name": "Python Worker",
    "port": 8080,
    "cwd": "/Users/me/projects/worker",
    "path": "/Users/me/projects/worker",
    "start": "python worker.py",
    "stop": "pkill -f worker.py",
    "restart": "",
    "status": "pgrep -f worker.py",
    "logs": "tail -n 160 worker.log",
    "config": "/Users/me/projects/worker/config.yaml"
  }
]
```

Fields:

- `id`: optional stable id used internally by Service Pilot. If omitted, it is derived from `name`.
- `name`: display name.
- `port`: optional single port.
- `ports`: optional list of ports.
- `cwd`: optional working directory for commands.
- `path`: optional path opened by Finder.
- `start`: command used by the Start action.
- `stop`: command used by the Stop action.
- `restart`: optional command used by Restart. If omitted or empty, Service Pilot runs `stop` and then `start`.
- `status`: optional command. Exit code `0` means running; non-zero means stopped. If omitted, status is `unknown`.
- `logs`: optional command used by the Logs panel.
- `config`: optional file path loaded and saved by the Config panel.

Custom commands run through `/bin/zsh -lc`.

You can add a custom service from the sidebar with `Add Custom`. Service Pilot writes one JSON file per service and refreshes the service list. Custom services are displayed newest first based on each service file's creation time.

## UI Behavior

- The left service list switches active state immediately after clicking another service.
- The right panel shows a short loading state while basic service content switches.
- Config is loaded automatically after a service is selected.
- Slower Homebrew details use local loading text in the Path and Version fields.
- Config and Logs can still be manually reloaded from their panel actions.

## Requirements

- macOS
- Docker Desktop, for Docker container discovery and control
- Homebrew, for Homebrew service discovery and control
- Custom service files, for user-defined command services
- Node.js and pnpm, for frontend development
- Rust and Cargo, for the Tauri backend

## Setup

Install Homebrew if needed:

```bash
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
```

Install Node.js:

```bash
brew install node
```

Enable pnpm:

```bash
corepack enable
corepack prepare pnpm@10.13.1 --activate
```

Install Rust:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
```

Install Docker Desktop if Docker support is needed:

```bash
brew install --cask docker
```

Open Docker Desktop once and wait until it is running before using Docker features.

## Development

Install dependencies:

```bash
pnpm install
```

Run the desktop app:

```bash
pnpm dev
```

Run frontend checks:

```bash
pnpm typecheck
pnpm web:build
```

Run backend checks:

```bash
cargo check --manifest-path src-tauri/Cargo.toml
```

Build the app:

```bash
pnpm build
```

## Troubleshooting

If Docker containers do not appear, make sure Docker Desktop is running and verify:

```bash
docker ps -a --format json
```

If Homebrew services do not appear, verify:

```bash
brew services list --json
```

If `pnpm dev` cannot find Cargo, reload your shell or source Rust's environment:

```bash
source "$HOME/.cargo/env"
```
