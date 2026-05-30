# taskforce

Personal task management system built around a Rust API, a web UI, and a local structured task store.

## Goals

- Keep the core repository focused on the product backend.
- Avoid a long-lived monorepo unless there is a clear operational need.
- Leave room for a separate web UI repository and infra repository.

## Repository role

This repository is intended to hold the main application backend and domain logic.

- Main product repository: `taskforce`
- Separate frontend repository: `taskforce-web`
- Optional infrastructure repository: `taskforce-infra`

## Planned structure

```text
taskforce/
├── README.md
├── AGENTS.md
├── Cargo.toml
├── .gitignore
├── src/
│   └── main.rs
├── crates/
│   ├── taskcore/
│   │   ├── Cargo.toml
│   │   └── src/
│   └── taskapi/
│       ├── Cargo.toml
│       └── src/
├── docs/
│   ├── architecture.md
│   └── api.md
└── scripts/
    └── dev/
```

## Notes

- Sync storage and Web UI details should be designed to work without forcing a monorepo.

## Local setup

- A sample config is available at `config/config.toml.sample`.
- A sample environment file is available at `config/taskforce.env.sample`.
- Copy it to `$XDG_CONFIG_HOME/taskforce/config.toml`, or `~/.config/taskforce/config.toml` if `XDG_CONFIG_HOME` is unset.
- For env-based overrides, copy `config/taskforce.env.sample` to `$XDG_CONFIG_HOME/taskforce/taskforce.env`.
- Taskforce always reads the base files first: `config.toml` and `taskforce.env`.
- If `TASKFORCE_ENV=<profile>` or `--env=<profile>` is set, taskforce also loads the matching profile entry from `[profiles]` / `[[profiles.items]]` in `config.toml`.
- Enabled plugin manifests are loaded only from `plugins/<plugin-id>/manifest.toml`.
- Catalog entries under `examples/plugins/` or `community/plugins/` are not loaded until you copy or symlink them into `plugins/`.
- The database backend is selected with `[backend].kind`; `sqlite` and `postgres` are supported.
- If `[backend].sqlite_path` and the legacy top-level `sqlite_path` are unset, taskforce uses `$XDG_DATA_HOME/taskforce/taskforce.db`, or `~/.local/share/taskforce/taskforce.db` if `XDG_DATA_HOME` is unset.
- For Postgres, set `[backend].postgres_url`, `TASKFORCE_POSTGRES_URL`, or the split env vars `TASKFORCE_POSTGRES_HOST` and `TASKFORCE_POSTGRES_PASS`. `TASKFORCE_POSTGRES_USER`, `TASKFORCE_POSTGRES_PORT`, `TASKFORCE_POSTGRES_DB`, and `TASKFORCE_POSTGRES_SSLMODE` are optional and default to `postgres`, `5432`, `postgres`, and `require`.
- When your provider requires a custom CA certificate, set `[backend].postgres_ssl_root_cert` or `TASKFORCE_POSTGRES_SSL_ROOT_CERT` to the PEM file path.
- Supabase works with a standard Postgres URL such as `postgresql://postgres:<password>@db.<project-ref>.supabase.co:5432/postgres?sslmode=require`, plus its downloadable CA certificate file when certificate verification needs an explicit root.

## Current commands

```bash
cargo run -- list
cargo run -- --env=development list
cargo run -- add "Write docs" --deadline 2026-06-05 --project taskforce --tag docs
cargo run -- add "Wait for design handoff" --status waiting --tag design
cargo run -- edit 1 "Write better docs" --target-date 2026-06-02 --launch-date 2026-06-10
cargo run -- edit 1 --clear-deadline --clear-project
cargo run -- set 1 requester ishii
cargo run -- get 1 requester
cargo run -- unset 1 requester
cargo run -- status 1
cargo run -- status 1 active
cargo run -- search --where "status in ('active', 'waiting')"
cargo run -- search --where "deadline between '2026-06-01' and '2026-06-30'" --where "chatwork.requester = '石井'"
cargo run -- import-chatwork "https://www.chatwork.com/#!rid36219958-2111786210627420160"
cargo run -- done 1
TASKFORCE_SQLITE_PATH="$HOME/.local/share/taskforce/taskforce.db" cargo run -- serve
TASKFORCE_POSTGRES_URL="postgresql://postgres:<password>@db.<project-ref>.supabase.co:5432/postgres?sslmode=require" cargo run -- serve
TASKFORCE_POSTGRES_HOST="db.<project-ref>.supabase.co" TASKFORCE_POSTGRES_USER="postgres" TASKFORCE_POSTGRES_PASS="<password>" TASKFORCE_POSTGRES_PORT="5432" TASKFORCE_POSTGRES_DB="postgres" cargo run -- serve
TASKFORCE_POSTGRES_SSL_ROOT_CERT="$HOME/.config/taskforce/supabase-prod-ca-2021.crt" cargo run -- serve
```

- `serve` binds to `127.0.0.1` and chooses a free port unless a host or port is configured.
- Manual runs load `taskforce.env` first, then the selected profile's `env_file` when `TASKFORCE_ENV` or `--env=<profile>` resolves to a configured profile.

## Development seed data

- `scripts/dev/seed_dummy_tasks.py` builds and runs a dev-only Rust bulk seeder.
- It defaults to `--env development --count 24 --seed 42 --start 1`.
- Each run uses `project = "seed-<seed>"` unless `--project` is given.
- Use `--start` to continue the same deterministic series from the middle without replacing the earlier rows.

```bash
scripts/dev/seed_dummy_tasks.py
scripts/dev/seed_dummy_tasks.py --env development --seed 77 --start 301 --count 300
target/debug/taskforce --env=development search --where "project = 'seed-77'" --where "status in ('active', 'waiting')"
```

## systemd user service

- Build a release binary before wiring the service:

```bash
cargo build --release
```

- If you want a stable local URL, set `[server].port` in your config before enabling the service.
- A sample user unit is available at `config/systemd/taskforce.service`.
- The sample unit reads `~/.config/taskforce/taskforce.env` via `EnvironmentFile=`. Set `TASKFORCE_ENV=production` there if you want the service to load the `production` profile from `config.toml`.
- Install it under `~/.config/systemd/user/taskforce.service`, then run:

```bash
systemctl --user daemon-reload
systemctl --user enable --now taskforce.service
systemctl --user status taskforce.service
```
