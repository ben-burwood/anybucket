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
