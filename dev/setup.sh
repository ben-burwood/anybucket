#!/usr/bin/env bash
#
# One-command local S3 for AnyBucket, powered by Garage.
#
# Brings up the Garage container, initialises the single-node cluster, imports
# fixed dev credentials, creates 5 buckets, seeds them with random files, and
# prints the connection details to paste into the app's "Garage" preset.
set -euo pipefail

export MSYS_NO_PATHCONV=1
export MSYS2_ARG_CONV_EXCL='*'

ENDPOINT_URL="http://localhost:3900"
REGION="garage"
ACCESS_KEY_ID="GKdeadbeefdeadbeefdeadbeef"
SECRET_ACCESS_KEY="deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef"
KEY_NAME="anybucket-dev"
BUCKETS="assets-dev logs-archive user-uploads media-library backups-daily"

cd "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

garage() { docker compose exec -T garage /garage "$@"; }

log() { printf '\n\033[1;36m==>\033[0m %s\n' "$*"; }

log "Starting Garage (docker compose up)"
docker compose up -d garage

log "Waiting for Garage to become ready"
for _ in $(seq 60); do
  garage status >/dev/null 2>&1 && ready=1 && break
  sleep 1
done
if [ -z "${ready:-}" ]; then
  echo "Garage did not become ready in time." >&2
  docker compose logs garage >&2
  exit 1
fi

# Assign a cluster layout
NODE_ID="$(garage node id -q 2>/dev/null | tr -d '\r' | cut -d@ -f1)"
if [ -z "$NODE_ID" ]; then
  echo "Could not determine Garage node id." >&2
  exit 1
fi

LAYOUT_VERSION="$(garage layout show 2>/dev/null \
  | sed -n 's/.*layout version: *\([0-9][0-9]*\).*/\1/p' | tail -1)"
if [ "${LAYOUT_VERSION:-0}" -gt 0 ]; then
  log "Cluster layout already assigned — skipping"
else
  log "Assigning cluster layout"
  garage layout assign -z dc1 -c 1G "$NODE_ID"
  garage layout apply --version "$(( ${LAYOUT_VERSION:-0} + 1 ))"
fi

if garage key info "$ACCESS_KEY_ID" >/dev/null 2>&1; then
  log "Access key already imported — skipping"
else
  log "Importing dev access key"
  garage key import -n "$KEY_NAME" "$ACCESS_KEY_ID" "$SECRET_ACCESS_KEY" --yes
fi

log "Creating buckets and granting access"
for b in $BUCKETS; do
  if garage bucket info "$b" >/dev/null 2>&1; then
    echo "  bucket '$b' already exists"
  else
    garage bucket create "$b"
    echo "  created bucket '$b'"
  fi
  garage bucket allow "$b" --key "$ACCESS_KEY_ID" --read --write --owner >/dev/null
done

log "Seeding buckets with random files"
docker compose run --rm --entrypoint /bin/sh -e BUCKETS="$BUCKETS" seed /seed.sh

printf '\n\033[1;32m==> Local S3 is ready.\033[0m\n'
cat <<EOF

In AnyBucket: Connections → New → pick the "Garage" preset, then paste:

  Endpoint URL      : ${ENDPOINT_URL}
  Region            : ${REGION}
  Force path style  : on (preset default)
  Access key ID     : ${ACCESS_KEY_ID}
  Secret access key : ${SECRET_ACCESS_KEY}

Set the connection mode to Read-Write (or Read-Write-Delete) to test uploads/deletes.

Buckets seeded: ${BUCKETS}

Manage the server:
  docker compose -f dev/compose.yml down      # stop (keeps data)
  docker compose -f dev/compose.yml down -v   # stop and wipe data
EOF
