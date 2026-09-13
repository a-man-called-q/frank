#!/bin/sh
set -eu

output_dir=${1:-docker/tls}
mkdir -p "$output_dir"

if [ -e "$output_dir/cert.pem" ] || [ -e "$output_dir/key.pem" ]; then
  printf '%s\n' "refusing to overwrite an existing certificate in $output_dir" >&2
  exit 1
fi

openssl req -x509 -newkey rsa:2048 -sha256 -nodes \
  -keyout "$output_dir/key.pem" \
  -out "$output_dir/cert.pem" \
  -days 825 \
  -subj "/CN=localhost" \
  -addext "basicConstraints=critical,CA:FALSE" \
  -addext "keyUsage=critical,digitalSignature,keyEncipherment" \
  -addext "extendedKeyUsage=serverAuth" \
  -addext "subjectAltName=DNS:localhost,IP:127.0.0.1,IP:::1"

# The daemon runs as the unprivileged `frank` user in Compose.  Development
# certificates are intentionally readable by that user; keep production keys
# outside this helper and provision them with permissions for the container
# UID/GID instead.
chmod 644 "$output_dir/key.pem"
chmod 644 "$output_dir/cert.pem"
printf '%s\n' "created development TLS identity in $output_dir"
