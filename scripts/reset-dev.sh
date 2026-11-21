#!/usr/bin/env bash
# Wipes the development database and data folder. Stop the server first.
set -euo pipefail
cd "$(dirname "$0")/.."
docker compose -f deploy/docker-compose.dev.yml exec -T postgres \
  psql -q -U fennoly -d fennoly -c 'DROP SCHEMA public CASCADE; CREATE SCHEMA public;'
rm -rf ./data
echo "Development database and ./data wiped."
