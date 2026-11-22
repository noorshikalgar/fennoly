#!/usr/bin/env bash
# Backup API checks that need no backup engine: access control, validation,
# secret handling, recovery keys, schedules and the "engine missing" errors.
#   scripts/backup-api-test.sh [base-url] [admin-user] [admin-password]
# Run against the sandbox after scripts/smoke.sh (which creates the admin).
# Expects restic NOT to be installed for the server (checks the 503 path);
# pass ENGINE=1 to skip those checks when it is.
set -euo pipefail

BASE="${1:-http://127.0.0.1:8687}"
USER_NAME="${2:-Noor}"
PASS="${3:-supersecret1}"
WORK="$(mktemp -d)"
ADMIN="$WORK/admin.jar"
MEMBER="$WORK/member.jar"
pass=0

ok() { pass=$((pass + 1)); printf '  ok  %s\n' "$1"; }
fail() { printf '  FAIL %s\n' "$1"; exit 1; }
jget() { python -c "import sys,json; d=json.load(sys.stdin); print(eval('d'+sys.argv[1]))" "$1"; }
# call <jar> <method> <path> [json] -> "<code>\n<body>"
call() {
  local jar=$1 method=$2 path=$3 body=${4:-}
  if [ -n "$body" ]; then
    curl -s -w '\n%{http_code}' -b "$jar" -c "$jar" -X "$method" -H 'content-type: application/json' -d "$body" "$BASE$path"
  else
    curl -s -w '\n%{http_code}' -b "$jar" -c "$jar" -X "$method" "$BASE$path"
  fi
}
code_of() { local out; out=$(call "$@"); printf '%s' "${out##*$'\n'}"; }
body_of() { local out; out=$(call "$@"); printf '%s' "${out%$'\n'*}"; }
expect() { # expect <code> <label> <call args...>
  local want=$1 label=$2; shift 2
  local got; got=$(code_of "$@")
  [ "$got" = "$want" ] || fail "$label: wanted HTTP $want, got $got ($(body_of "$@"))"
  ok "$label"
}

echo "Backup API test against $BASE"
call "$ADMIN" POST /api/auth/login "{\"username\":\"$USER_NAME\",\"password\":\"$PASS\"}" > /dev/null
[ "$(code_of "$ADMIN" GET /api/me)" = 200 ] || fail "admin login"

# A plain member must not see or touch backups.
M="bk$RANDOM"
call "$ADMIN" POST /api/admin/users "{\"username\":\"$M\",\"display_name\":\"Backup Test\",\"password\":\"supersecret3\"}" > /dev/null
call "$MEMBER" POST /api/auth/login "{\"username\":\"$M\",\"password\":\"supersecret3\"}" > /dev/null
expect 403 "member cannot list backups" "$MEMBER" GET /api/admin/backups
expect 403 "member cannot add a destination" "$MEMBER" POST /api/admin/backups '{"name":"x","kind":"local","settings":{"path":"/tmp/x"}}'
expect 403 "member cannot read backup status" "$MEMBER" GET /api/admin/backups/status
expect 401 "signed-out visitor is refused" "$WORK/none.jar" GET /api/admin/backups

LIST=$(body_of "$ADMIN" GET /api/admin/backups)
[ "$(echo "$LIST" | jget "['engine']['restic']")" = "False" ] || [ "${ENGINE:-}" = 1 ] || fail "restic reported as installed"
ok "engine availability is reported"

# --- validation -------------------------------------------------------------
if [[ "$(uname -s)" == MINGW* || "$(uname -s)" == MSYS* ]]; then
  OUTSIDE='C:\\FennolyBackupTest'
  INSIDE="$(cd "$(dirname "$0")/.." && pwd -W)/data-test/backups"
else
  OUTSIDE='/tmp/fennoly-backup-test'
  INSIDE="$(cd "$(dirname "$0")/.." && pwd)/data-test/backups"
fi
local_body() { printf '{"name":"%s","kind":"local","settings":{"path":"%s"}%s}' "$1" "$2" "${3:-}"; }

expect 400 "empty name is refused" "$ADMIN" POST /api/admin/backups "$(local_body '  ' "$OUTSIDE")"
expect 400 "very long name is refused" "$ADMIN" POST /api/admin/backups "$(local_body "$(printf 'x%.0s' {1..81})" "$OUTSIDE")"
expect 400 "unknown type is refused" "$ADMIN" POST /api/admin/backups '{"name":"x","kind":"ftp","settings":{}}'
expect 400 "relative folder is refused" "$ADMIN" POST /api/admin/backups "$(local_body x 'backups')"
expect 400 "folder inside the data folder is refused" "$ADMIN" POST /api/admin/backups "$(local_body x "$INSIDE")"
expect 400 "the data folder's parent is refused" "$ADMIN" POST /api/admin/backups "$(local_body x "$(dirname "$INSIDE")/..")"
expect 400 "bad schedule is refused" "$ADMIN" POST /api/admin/backups "$(local_body x "$OUTSIDE" ',"schedule":"hourly"')"
expect 400 "bad time is refused" "$ADMIN" POST /api/admin/backups "$(local_body x "$OUTSIDE" ',"schedule":"daily 24:10"')"
expect 400 "keeping nothing is refused" "$ADMIN" POST /api/admin/backups "$(local_body x "$OUTSIDE" ',"keep_daily":0,"keep_weekly":0,"keep_monthly":0')"
expect 400 "negative retention is refused" "$ADMIN" POST /api/admin/backups "$(local_body x "$OUTSIDE" ',"keep_daily":-1')"
expect 400 "share without a password is refused" "$ADMIN" POST /api/admin/backups '{"name":"nas","kind":"smb","settings":{"host":"nas","share":"backup","user":"noor"}}'
expect 400 "blank password counts as missing" "$ADMIN" POST /api/admin/backups '{"name":"nas","kind":"smb","settings":{"host":"nas","share":"backup","user":"noor"},"secrets":{"password":"   "}}'
expect 400 "S3 endpoint must be http(s)" "$ADMIN" POST /api/admin/backups '{"name":"s3","kind":"s3","settings":{"endpoint":"ftp://x","bucket":"b"},"secrets":{"access_key":"a","secret_key":"b"}}'
expect 400 "control characters are refused" "$ADMIN" POST /api/admin/backups '{"name":"nas","kind":"sftp","settings":{"host":"nas\nevil","user":"u","path":"/b"},"secrets":{"password":"p"}}'
expect 422 "malformed body is refused" "$ADMIN" POST /api/admin/backups '{"name":1}'

# --- create -------------------------------------------------------------------
OUT=$(body_of "$ADMIN" POST /api/admin/backups "$(local_body 'USB drive' "$OUTSIDE")")
LOCAL_ID=$(echo "$OUT" | jget "['target']['id']")
KEY=$(echo "$OUT" | jget "['recovery_key']")
[[ "$KEY" =~ ^[0-9A-HJKMNP-TV-Z]{4}(-[0-9A-HJKMNP-TV-Z]{4}){7}$ ]] || fail "recovery key format: $KEY"
ok "encrypted destination returns a recovery key once"
[ "$(echo "$OUT" | jget "['target']['encrypted']")" = "True" ] || fail "encrypted by default"
[ "$(echo "$OUT" | jget "['target']['next_run_at']")" != "None" ] || fail "next run scheduled"
ok "encrypted and scheduled by default"

SMB_PASS="s3cr3t-$RANDOM-पासवर्ड"
OUT=$(body_of "$ADMIN" POST /api/admin/backups "{\"name\":\"NAS\",\"kind\":\"smb\",\"encrypted\":false,\"settings\":{\"host\":\"192.168.1.20\",\"share\":\"backup\",\"path\":\"fennoly\",\"user\":\"noor\"},\"secrets\":{\"password\":\"$SMB_PASS\"},\"schedule\":\"weekly sun 03:30\"}")
SMB_ID=$(echo "$OUT" | jget "['target']['id']")
[ "$(echo "$OUT" | jget "['recovery_key']")" = "None" ] || fail "unencrypted has no key"
[ "$(echo "$OUT" | jget "['target']['has_password']")" = "True" ] || fail "password stored"
[ "$(echo "$OUT" | jget "['target']['location']")" = '\\192.168.1.20\backup\fennoly' ] || fail "location: $(echo "$OUT" | jget "['target']['location']")"
ok "network share saved with a readable location"

LIST=$(body_of "$ADMIN" GET /api/admin/backups)
if grep -q "$SMB_PASS\|$KEY" <<<"$LIST"; then fail "a secret leaked into the listing"; fi
if grep -q "$SMB_PASS\|$KEY" <<<"$(body_of "$ADMIN" GET "/api/admin/backups/$SMB_ID")"; then fail "secret leaked"; fi
ok "passwords and keys never come back from the API"

# Stored encrypted at rest (needs the dev Postgres container).
if command -v docker > /dev/null; then
  ROW=$(docker compose -f "$(dirname "$0")/../deploy/docker-compose.dev.yml" exec -T postgres \
    psql -At -U fennoly -d fennoly_test -c "SELECT secrets || ' ' || coalesce(repo_password, '-') FROM backup_targets WHERE id IN ('$SMB_ID', '$LOCAL_ID')" 2>/dev/null || true)
  if [ -n "$ROW" ]; then
    if grep -q "$SMB_PASS\|$KEY" <<<"$ROW"; then fail "secret stored in plain text"; fi
    grep -q '^v1:' <<<"$ROW" || fail "secrets not sealed: $ROW"
    ok "secrets are encrypted in the database"
  fi
fi

# --- update -------------------------------------------------------------------
OUT=$(body_of "$ADMIN" PATCH "/api/admin/backups/$SMB_ID" '{"name":"Home NAS","secrets":{"password":""}}')
[ "$(echo "$OUT" | jget "['name']")" = "Home NAS" ] || fail "rename"
[ "$(echo "$OUT" | jget "['has_password']")" = "True" ] || fail "blank password must keep the stored one"
ok "rename keeps the stored password"
OUT=$(body_of "$ADMIN" PATCH "/api/admin/backups/$SMB_ID" '{"enabled":false}')
[ "$(echo "$OUT" | jget "['next_run_at']")" = "None" ] || fail "paused has no next run"
OUT=$(body_of "$ADMIN" PATCH "/api/admin/backups/$SMB_ID" '{"enabled":true,"schedule":"daily 01:15"}')
[ "$(echo "$OUT" | jget "['schedule']")" = "daily 01:15" ] || fail "schedule"
[ "$(echo "$OUT" | jget "['next_run_at']")" != "None" ] || fail "resumed has a next run"
ok "pause, resume and reschedule"
expect 400 "update validates the location" "$ADMIN" PATCH "/api/admin/backups/$SMB_ID" '{"settings":{"host":"","share":"x","user":"u"}}'
expect 400 "update validates retention" "$ADMIN" PATCH "/api/admin/backups/$SMB_ID" '{"keep_daily":0,"keep_weekly":0,"keep_monthly":0}'
expect 400 "local folder cannot move into the data folder" "$ADMIN" PATCH "/api/admin/backups/$LOCAL_ID" "{\"settings\":{\"path\":\"$INSIDE\"}}"
expect 404 "unknown destination" "$ADMIN" PATCH /api/admin/backups/00000000-0000-0000-0000-000000000000 '{"name":"x"}'

# --- recovery key -------------------------------------------------------------
expect 400 "recovery key needs the right password" "$ADMIN" POST "/api/admin/backups/$LOCAL_ID/recovery-key" '{"password":"wrong-one"}'
AGAIN=$(body_of "$ADMIN" POST "/api/admin/backups/$LOCAL_ID/recovery-key" "{\"password\":\"$PASS\"}" | jget "['recovery_key']")
[ "$AGAIN" = "$KEY" ] || fail "recovery key mismatch"
ok "recovery key can be shown again with the password"
[ "$(body_of "$ADMIN" POST "/api/admin/backups/$SMB_ID/recovery-key" "{\"password\":\"$PASS\"}" | jget "['recovery_key']")" = "None" ] || fail "unencrypted key"
ok "unencrypted destination has no key"

# Connecting to an existing encrypted backup with its own key.
OUT=$(body_of "$ADMIN" POST /api/admin/backups "{\"name\":\"Old USB\",\"kind\":\"local\",\"settings\":{\"path\":\"${OUTSIDE}2\"},\"recovery_key\":\"my-old-key\"}")
OLD_ID=$(echo "$OUT" | jget "['target']['id']")
[ "$(echo "$OUT" | jget "['recovery_key']")" = "None" ] || fail "given key must not be echoed"
[ "$(body_of "$ADMIN" POST "/api/admin/backups/$OLD_ID/recovery-key" "{\"password\":\"$PASS\"}" | jget "['recovery_key']")" = "my-old-key" ] || fail "given key stored"
ok "an existing backup's key can be supplied"

# --- engine-dependent actions -------------------------------------------------
SNAP=0123456789abcdef
if [ "${ENGINE:-}" != 1 ]; then
  expect 503 "test needs the engine" "$ADMIN" POST "/api/admin/backups/$LOCAL_ID/test"
  expect 503 "back up now needs the engine" "$ADMIN" POST "/api/admin/backups/$LOCAL_ID/run"
  expect 503 "listing backups needs the engine" "$ADMIN" GET "/api/admin/backups/$LOCAL_ID/snapshots"
  expect 503 "restore needs the engine" "$ADMIN" POST "/api/admin/backups/$LOCAL_ID/snapshots/$SNAP/restore" '{"paths":["family"]}'
fi
expect 400 "bad snapshot id" "$ADMIN" GET "/api/admin/backups/$LOCAL_ID/snapshots/zz;rm/ls"
expect 400 "path cannot climb out" "$ADMIN" GET "/api/admin/backups/$LOCAL_ID/snapshots/$SNAP/ls?path=family/../../etc"
expect 400 "absolute drive paths are refused" "$ADMIN" GET "/api/admin/backups/$LOCAL_ID/snapshots/$SNAP/ls?path=C:/Windows"
[ "$(body_of "$ADMIN" GET "/api/admin/backups/$LOCAL_ID/runs")" = "[]" ] || fail "no runs yet"
ok "no runs recorded for refused actions"

STATUS=$(body_of "$ADMIN" GET /api/admin/backups/status)
[ "$(echo "$STATUS" | jget "['stale']")" = "False" ] || fail "new destinations are not stale yet"
[ "$(echo "$STATUS" | jget "['targets']")" -ge 3 ] || fail "status counts"
ok "status reports destinations"

# --- delete -------------------------------------------------------------------
for id in "$LOCAL_ID" "$SMB_ID" "$OLD_ID"; do
  expect 200 "delete destination" "$ADMIN" DELETE "/api/admin/backups/$id"
done
expect 404 "deleted destination is gone" "$ADMIN" DELETE "/api/admin/backups/$LOCAL_ID"
expect 404 "its runs are gone" "$ADMIN" GET "/api/admin/backups/$LOCAL_ID/runs"

echo "All $pass backup API checks passed."
