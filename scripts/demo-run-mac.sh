#!/usr/bin/env bash
set -euo pipefail

if [[ "$(uname -s)" != 'Darwin' ]]; then
  echo 'demo:run-mac must be run on macOS' >&2
  exit 2
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
app_dir="$repo_root/apps/frank_desktop"
tls_dir="$repo_root/docker/tls"
env_file="$app_dir/.env"
state_dir="$repo_root/.frank-preview/demo-mac"
pid_file="$state_dir/flutter.pid"
project_name='frank-demo-mac'
base_url='https://127.0.0.1:37465'
db_path='/var/lib/frank/frank.sqlite3'
status_file=''
flutter_pid=''

for command_name in docker curl openssl base64 python3 proto; do
  if ! command -v "$command_name" >/dev/null 2>&1; then
    echo "demo:run-mac requires $command_name" >&2
    exit 2
  fi
done

if ! docker info >/dev/null 2>&1; then
  echo 'demo:run-mac cannot reach the Docker daemon' >&2
  exit 2
fi

mkdir -p "$tls_dir" "$state_dir"

if [[ ! -s "$tls_dir/cert.pem" || ! -s "$tls_dir/key.pem" ]]; then
  "$repo_root/scripts/dev-cert.sh" "$tls_dir"
fi

if [[ -f "$pid_file" ]]; then
  old_pid="$(cat "$pid_file" 2>/dev/null || true)"
  old_command=''
  if [[ "$old_pid" =~ ^[0-9]+$ ]] && kill -0 "$old_pid" 2>/dev/null; then
    old_command="$(ps -p "$old_pid" -o command= 2>/dev/null || true)"
  fi
  if [[ "$old_command" == *flutter* || "$old_command" == *flutter_tools* ]]; then
    echo "Frank macOS demo is already running (Flutter pid $old_pid)"
    exit 0
  fi
  rm -f "$pid_file"
fi

export FRANK_TLS_DIR="$tls_dir"
# This task is intentionally local-only. The regular Compose workflow still
# supports an explicit LAN publish address for deployments outside this demo.
export FRANK_PUBLISH_HOST='127.0.0.1'
compose=(docker compose --project-name "$project_name")

echo 'starting the Frank server container'
"${compose[@]}" up -d --build frankd

wait_for_health() {
  local attempt=0
  local health_code
  while ((attempt < 60)); do
    health_code="$(curl --silent --cacert "$tls_dir/cert.pem" \
      --connect-timeout 2 --max-time 3 --output /dev/null \
      --write-out '%{http_code}' "$base_url/v2/health" || true)"
    if [[ "$health_code" == '200' ]]; then
      return 0
    fi
    attempt=$((attempt + 1))
    sleep 1
  done
  echo 'Frank server did not become healthy over HTTPS' >&2
  return 1
}

wait_for_health

status_file="$(mktemp "${TMPDIR:-/tmp}/frank-demo-auth.XXXXXX")"
curl --silent --show-error --cacert "$tls_dir/cert.pem" \
  --output "$status_file" "$base_url/v2/auth/status"
configured="$(python3 - "$status_file" <<'PY'
import json
import sys

with open(sys.argv[1], encoding='utf-8') as handle:
    payload = json.load(handle)
print('true' if payload.get('configured') is True else 'false')
PY
)"
rm -f "$status_file"
status_file=''

if [[ "$configured" != 'true' ]]; then
  cat <<'MESSAGE'
No Frank owner account exists for the demo volume yet.
The server will stop briefly and prompt for the owner username/password.
MESSAGE
  "${compose[@]}" stop frankd
  "${compose[@]}" run --rm --no-deps frankd auth init --db "$db_path"
  "${compose[@]}" up -d frankd
  wait_for_health
fi

ca_base64="$(base64 < "$tls_dir/cert.pem" | tr -d '\n')"
python3 - "$env_file" "$ca_base64" "$base_url" <<'PY'
import json
import os
import pathlib
import sys
import tempfile

env_path = pathlib.Path(sys.argv[1])
ca_certificate = sys.argv[2]
values = {}
if env_path.exists():
    try:
        with env_path.open(encoding='utf-8') as handle:
            loaded = json.load(handle)
        if not isinstance(loaded, dict):
            raise ValueError('the file must contain a JSON object')
        values.update(loaded)
    except (OSError, json.JSONDecodeError, ValueError) as error:
        raise SystemExit(f'cannot update {env_path}: {error}')

values['FRANK_SERVER_URL'] = sys.argv[3]
values['FRANK_SERVER_CA_PEM_BASE64'] = ca_certificate
env_path.parent.mkdir(parents=True, exist_ok=True)
fd, temporary = tempfile.mkstemp(prefix=f'.{env_path.name}.', dir=env_path.parent)
try:
    with os.fdopen(fd, 'w', encoding='utf-8') as handle:
        json.dump(values, handle, indent=2)
        handle.write('\n')
    os.chmod(temporary, 0o600)
    os.replace(temporary, env_path)
except BaseException:
    try:
        os.unlink(temporary)
    except FileNotFoundError:
        pass
    raise
PY

echo "wrote local Flutter config to $env_file (ignored by git)"
echo 'starting the Flutter macOS client; use demo:stop-mac to stop the server and client'

(
  cd "$app_dir"
  exec proto run flutter -- run -d macos --dart-define-from-file=.env
) &
flutter_pid=$!
printf '%s\n' "$flutter_pid" > "$pid_file"

cleanup() {
  local exit_code=$?
  if [[ -n "$status_file" ]]; then
    rm -f "$status_file"
  fi
  if [[ -n "$flutter_pid" ]] && kill -0 "$flutter_pid" 2>/dev/null; then
    kill -TERM "$flutter_pid" 2>/dev/null || true
  fi
  rm -f "$pid_file"
  exit "$exit_code"
}
trap cleanup EXIT
trap 'exit 130' INT TERM

set +e
wait "$flutter_pid"
flutter_status=$?
set -e
exit "$flutter_status"
