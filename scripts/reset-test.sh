#!/usr/bin/env bash
# Wipes the sandbox test database (fennoly_test) and ./data-test.
# Never touches the main development data. Stop the test server first.
set -euo pipefail
cd "$(dirname "$0")/.."
docker compose -f deploy/docker-compose.dev.yml exec -T postgres \
  psql -q -U fennoly -d fennoly_test -c 'DROP SCHEMA public CASCADE; CREATE SCHEMA public;'
rm -rf ./data-test
echo "Test database and ./data-test wiped."
