#!/bin/sh
set -eu

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$repo_root"

for command_name in docker curl python3; do
  if ! command -v "$command_name" >/dev/null 2>&1; then
    printf 'auth Docker smoke requires %s\n' "$command_name" >&2
    exit 1
  fi
done

project="frank-auth-smoke-$(date +%s)-$$"
temp_root=$(mktemp -d "${TMPDIR:-/tmp}/frank-auth-smoke.XXXXXX")
tls_dir="$temp_root/tls"
responses_dir="$temp_root/responses"
mkdir -p "$tls_dir" "$responses_dir"

compose_started=false
cleanup() {
  if [ "$compose_started" = true ]; then
    # The project name is unique to this invocation, so its volume is test
    # data owned by this smoke test and can be removed on every exit path.
    docker compose -p "$project" down --volumes --remove-orphans >/dev/null 2>&1 || true
  fi
  rm -rf "$temp_root"
}
trap cleanup EXIT HUP INT TERM

fail() {
  printf 'auth Docker smoke failed: %s\n' "$*" >&2
  exit 1
}

export FRANK_TLS_DIR="$tls_dir"
base_url='https://127.0.0.1:37465'
username='smokeowner'
# This value is supplied only through the hidden CLI prompt's stdin and is
# discarded with the temporary test directory after the run.
password="frank-smoke-password-$(date +%s)-$$"
login_response="$responses_dir/login.json"
response="$responses_dir/response.json"
auth_header="$responses_dir/auth-header"

printf '%s\n' 'generating an isolated development certificate'
"$repo_root/scripts/dev-cert.sh" "$tls_dir" >/dev/null

printf '%s\n' 'building the pinned Frank Docker image'
docker compose -p "$project" build frankd >/dev/null

printf '%s\n' 'bootstrapping the owner through the one-off container'
printf '%s\n%s\n' "$password" "$password" |
  docker compose -p "$project" run --rm --no-deps frankd auth init \
    --db /var/lib/frank/frank.sqlite3 --username "$username" >/dev/null

if printf '%s\n%s\n' "$password" "$password" |
  docker compose -p "$project" run --rm --no-deps frankd auth init \
    --db /var/lib/frank/frank.sqlite3 --username "$username" \
    >"$responses_dir/second-init.log" 2>&1; then
  fail 'a second owner bootstrap unexpectedly succeeded'
fi
if ! grep -q 'already initialized' "$responses_dir/second-init.log"; then
  fail 'a second owner bootstrap returned an unexpected error'
fi

printf '%s\n' 'starting frankd and waiting for HTTPS health'
compose_started=true
docker compose -p "$project" up -d frankd >/dev/null

ready=false
attempt=0
while [ "$attempt" -lt 60 ]; do
  if health_code=$(curl --silent --cacert "$tls_dir/cert.pem" \
    --connect-timeout 2 --max-time 3 --output /dev/null \
    --write-out '%{http_code}' "$base_url/v2/health"); then
    if [ "$health_code" = 200 ]; then
      ready=true
      break
    fi
  fi
  attempt=$((attempt + 1))
  sleep 1
done
[ "$ready" = true ] || fail 'frankd did not become healthy over HTTPS'

status_code=$(curl --silent --show-error --cacert "$tls_dir/cert.pem" \
  --output "$response" --write-out '%{http_code}' "$base_url/v2/auth/status")
[ "$status_code" = 200 ] || fail "auth status returned HTTP $status_code"
python3 - "$response" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as handle:
    status = json.load(handle)
if status.get("configured") is not True or status.get("auth_method") != "local-password":
    raise SystemExit("owner setup status was not configured for local password auth")
PY

printf '%s\n' 'logging in and validating the persisted session'
login_code=$(printf '{"username":"%s","password":"%s","device_name":"Docker smoke"}' \
  "$username" "$password" |
  curl --silent --show-error --cacert "$tls_dir/cert.pem" \
    --header 'content-type: application/json' --data-binary @- \
    --output "$login_response" --write-out '%{http_code}' \
    "$base_url/v2/auth/login")
[ "$login_code" = 200 ] || fail "login returned HTTP $login_code"

token=$(python3 - "$login_response" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as handle:
    response = json.load(handle)
token = response.get("access_token")
if not isinstance(token, str) or not token:
    raise SystemExit("login response did not contain an access token")
print(token, end="")
PY
) || fail 'could not read the access token from the login response'
printf 'Authorization: Bearer %s\n' "$token" >"$auth_header"

auth_code=$(curl --silent --show-error --cacert "$tls_dir/cert.pem" \
  --header "@$auth_header" --output "$response" --write-out '%{http_code}' \
  "$base_url/v2/auth/me")
[ "$auth_code" = 200 ] || fail "initial /auth/me returned HTTP $auth_code"

printf '%s\n' 'restarting frankd and checking session persistence'
docker compose -p "$project" restart frankd >/dev/null
ready=false
attempt=0
while [ "$attempt" -lt 60 ]; do
  if health_code=$(curl --silent --cacert "$tls_dir/cert.pem" \
    --connect-timeout 2 --max-time 3 --output /dev/null \
    --write-out '%{http_code}' "$base_url/v2/health"); then
    if [ "$health_code" = 200 ]; then
      ready=true
      break
    fi
  fi
  attempt=$((attempt + 1))
  sleep 1
done
[ "$ready" = true ] || fail 'frankd did not become healthy after restart'

auth_code=$(curl --silent --show-error --cacert "$tls_dir/cert.pem" \
  --header "@$auth_header" --output "$response" --write-out '%{http_code}' \
  "$base_url/v2/auth/me")
[ "$auth_code" = 200 ] || fail "persisted /auth/me returned HTTP $auth_code"

printf '%s\n' 'logging out and confirming server-side revocation'
logout_code=$(curl --silent --show-error --cacert "$tls_dir/cert.pem" \
  --header "@$auth_header" --request POST --output "$response" \
  --write-out '%{http_code}' "$base_url/v2/auth/logout")
[ "$logout_code" = 200 ] || fail "logout returned HTTP $logout_code"

revoked_code=$(curl --silent --show-error --cacert "$tls_dir/cert.pem" \
  --header "@$auth_header" --output "$response" --write-out '%{http_code}' \
  "$base_url/v2/auth/me")
[ "$revoked_code" = 401 ] || fail "revoked /auth/me returned HTTP $revoked_code"

printf '%s\n' 'auth Docker smoke passed'
