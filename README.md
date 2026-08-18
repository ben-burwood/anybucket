# AnyBucket

Explorer for **S3-compatible object stores** — AWS S3, [RustFS](https://rustfs.com/), [Garage](https://garagehq.deuxfleurs.fr/)... 

Built with **Tauri + Vue**, using [`aws-sdk-s3`](https://docs.rs/aws-sdk-s3/latest/aws_sdk_s3/) and [`ag-grid`](https://www.ag-grid.com/).

## Features

- **Connections** — save multiple S3 endpoints. Secrets are stored in the OS keychain.
- **Bucket list** → **folder-style browser**
- **Per-object actions** — copy `s3://` URI, copy HTTPS URL, generate a presigned GET URL, and stream-download to disk.

## Prerequisites

- **Node.js** 18+ and **Rust** 1.94.1+ (stable).
- **Windows only:** the AWS SDK's default crypto (`aws-lc-sys`) compiles native code —
  you need **MSVC Build Tools**, **NASM**, and **CMake** on `PATH`.

## Develop

```bash
npm install
npm run tauri dev      # launches the desktop app with hot reload
```

## Local dev S3

Need something to point the app at? [`dev/`](dev/) spins up a local **Garage** S3 server in Docker, pre-seeded with 5 buckets of random files: `./dev/setup.sh`

It prints ready-to-paste connection details for the app's built-in **Garage** preset. See [`dev/README.md`](dev/README.md) for details.

## Self-hosted web app (Docker)

The same UI also runs as a self-hosted web app in a single container — an `axum` server serves the SPA and performs S3 operations server-side (the browser never talks to S3 directly), sharing the Rust core with the desktop build.

```bash
cp .env.example .env      # set ANYBUCKET_MASTER_KEY
docker compose up -d
```

It has **no built-in auth** and must run behind a reverse proxy providing auth + TLS!

### Configuration

All configuration is via environment variables:

| Variable               | Required | Default      | Purpose                                                        |
|------------------------|----------|--------------|----------------------------------------------------------------|
| `ANYBUCKET_MASTER_KEY` | **yes**  | —            | Encrypts stored S3 secret keys at rest. Server refuses to start without it. |
| `ANYBUCKET_CONFIG_DIR` | no       | `/config`    | Where `connections.json` + `secrets.json` live.                |
| `ANYBUCKET_STATIC_DIR` | no       | `/app/dist`  | The built SPA served as static files.                          |
| `ANYBUCKET_PORT`       | no       | `8080`       | Port the server listens on inside the container.               |

### Data & backups

The `/config` volume (named `anybucket-config` in compose) holds:

- `connections.json` — connection metadata (endpoint, region, access key id, mode). **Plaintext.**
- `secrets.json` — the S3 secret access keys, **AES-256-GCM encrypted** under `ANYBUCKET_MASTER_KEY`.

Back up **both the volume and the master key**. Restoring the volume without the exact master key leaves the secrets unreadable (you'd have to re-enter them).
