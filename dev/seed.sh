#!/bin/sh
# Seed each bucket in $BUCKETS with a light spread of small random files.
#
# Runs inside the aws-cli container (see compose.yml "seed" service).
set -eu

ENDPOINT="http://garage:3900"
: "${BUCKETS:?BUCKETS env var must be set}"

# Garage needs path-style addressing (bucket in the URL path, not the host)
aws configure set default.s3.addressing_style path

# 1x1 transparent PNG so the object list shows a real image type.
PNG_B64="iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg=="

for b in $BUCKETS; do
  echo "  seeding $b ..."
  root="/tmp/seed/$b"
  mkdir -p "$root/notes" "$root/config" "$root/data/2024/q1" \
           "$root/images/thumbnails" "$root/assets/raw" "$root/logs"

  # Plain text
  head -c 2048 /dev/urandom | base64 > "$root/README.txt"
  printf 'app=%s\nseeded=true\nrun=dev\n' "$b" > "$root/notes/INFO.txt"

  # JSON + CSV
  printf '{"bucket":"%s","env":"dev","items":[1,2,3]}\n' "$b" > "$root/config/settings.json"
  printf 'id,name,value\n1,alpha,%s\n2,beta,%s\n' "$RANDOM" "$RANDOM" > "$root/data/records.csv"
  printf 'quarter,total\nQ1,%s\nQ2,%s\n' "$RANDOM" "$RANDOM" > "$root/data/2024/q1/summary.csv"

  # PNG
  printf '%s' "$PNG_B64" | base64 -d > "$root/images/thumbnails/pixel.png"

  # Random Binary Blobs
  dd if=/dev/urandom bs=1024 count=4 2>/dev/null > "$root/assets/blob-1.bin"
  dd if=/dev/urandom bs=1024 count=8 2>/dev/null > "$root/assets/raw/blob-2.bin"

  for n in 1 2 3 4 5; do
    printf '2024-01-0%s INFO event %s in %s\n' "$n" "$RANDOM" "$b"
  done > "$root/logs/app.log"

  aws --endpoint-url "$ENDPOINT" s3 cp "$root" "s3://$b/" --recursive >/dev/null
done

echo "  seeding complete."
