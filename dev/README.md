# Local dev S3 (Garage)

A one-command, throwaway S3-compatible server for developing AnyBucket against —
no AWS account, no real credentials. Powered by [Garage](https://garagehq.deuxfleurs.fr/)
in Docker.

## Prerequisites

- Docker + Docker Compose (Docker Desktop on macOS/Windows is fine).
- A Bash shell to run `setup.sh`: macOS/Linux, WSL, or **Git Bash** on Windows.

## Quick start

```bash
./dev/setup.sh
```

That will:

1. Start the Garage container (`docker compose up -d garage`).
2. Initialise the single-node cluster layout.
3. Import a **fixed dev access key**.
4. Create 5 buckets: `assets-dev`, `logs-archive`, `user-uploads`, `media-library`, `backups-daily`.
5. Seed each bucket with ~9 small random files (text/json/csv/png/binary), some under
   nested folders like `data/2024/q1/` and `images/thumbnails/` to exercise the folder browser.
6. Print the connection details below.

The script is **idempotent** — re-run it any time (e.g. to re-seed).

## Connect the app

In AnyBucket: **Connections → New → "Garage" preset**, then paste:

| Field             | Value                                                              |
|-------------------|-------------------------------------------------------------------|
| Endpoint URL      | `http://localhost:3900`                                            |
| Region            | `garage`                                                          |
| Force path style  | on (preset default)                                               |
| Access key ID     | `GKdeadbeefdeadbeefdeadbeef`                                       |
| Secret access key | `deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef` |

The endpoint/region/path-style already match the app's built-in **Garage** preset, so you
only need to paste the key ID + secret. Set the connection **mode** to Read-Write (or
Read-Write-Delete) if you want to test uploads/copies/deletes.

> **These credentials are well-known and dev-only.** They live in `compose.yml`,
> `garage.toml` and `setup.sh` on purpose so the environment is reproducible. Never reuse
> them anywhere real.

## Managing the server

```bash
docker compose -f dev/compose.yml up -d garage   # start
docker compose -f dev/compose.yml logs -f garage # tail logs
docker compose -f dev/compose.yml down           # stop (keeps data)
docker compose -f dev/compose.yml down -v        # stop and WIPE all data
```

Data persists in the `garage_meta` / `garage_data` Docker volumes across restarts. After a
`down -v` wipe, just run `./dev/setup.sh` again.

## Poke at it without the app

```bash
# List objects in a bucket (via the bundled aws-cli tooling container)
docker compose -f dev/compose.yml run --rm --entrypoint /bin/sh seed -c \
  'aws configure set default.s3.addressing_style path && \
   aws --endpoint-url http://garage:3900 s3 ls s3://assets-dev --recursive'

# Garage admin CLI
docker compose -f dev/compose.yml exec garage /garage bucket list
docker compose -f dev/compose.yml exec garage /garage status
```

## Files

| File                 | Purpose                                                          |
|----------------------|------------------------------------------------------------------|
| `compose.yml` | Garage server + a one-shot `aws-cli` tooling service for seeding. |
| `garage.toml`        | Single-node Garage config (ports, region, dev secrets).          |
| `setup.sh`           | Orchestrates bring-up, cluster init, key/bucket creation, seeding.|
| `seed.sh`            | Generates + uploads the random seed objects (runs in aws-cli).    |

## Ports

| Port | Service                                        |
|------|------------------------------------------------|
| 3900 | S3 API — what the app connects to.             |
| 3903 | Garage admin API.                              |
