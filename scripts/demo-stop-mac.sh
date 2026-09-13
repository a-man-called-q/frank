#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
state_dir="$repo_root/.frank-preview/demo-mac"
pid_file="$state_dir/flutter.pid"
project_name='frank-demo-mac'

if [[ -f "$pid_file" ]]; then
  flutter_pid="$(cat "$pid_file" 2>/dev/null || true)"
  flutter_command=''
  if [[ "$flutter_pid" =~ ^[0-9]+$ ]] && kill -0 "$flutter_pid" 2>/dev/null; then
    flutter_command="$(ps -p "$flutter_pid" -o command= 2>/dev/null || true)"
  fi
  if [[ "$flutter_command" == *flutter* || "$flutter_command" == *flutter_tools* ]]; then
    echo "stopping Flutter macOS client (pid $flutter_pid)"
    kill -TERM "$flutter_pid" 2>/dev/null || true
    for _ in {1..50}; do
      kill -0 "$flutter_pid" 2>/dev/null || break
      sleep 0.1
    done
    if kill -0 "$flutter_pid" 2>/dev/null; then
      kill -KILL "$flutter_pid" 2>/dev/null || true
    fi
  fi
  rm -f "$pid_file"
else
  echo 'no tracked Flutter demo process found'
fi

if ! command -v docker >/dev/null 2>&1; then
  echo 'demo:stop-mac requires docker' >&2
  exit 2
fi
if ! docker info >/dev/null 2>&1; then
  echo 'demo:stop-mac cannot reach the Docker daemon' >&2
  exit 2
fi

export FRANK_TLS_DIR="$repo_root/docker/tls"
export FRANK_PUBLISH_HOST='127.0.0.1'
echo 'stopping the Frank demo server container'
docker compose --project-name "$project_name" down --remove-orphans
echo 'Frank macOS demo stopped; the named volume was preserved'
