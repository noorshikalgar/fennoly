#!/usr/bin/env bash
# Full backup and restore drill against the Docker test stack in
# deploy/backup-test (real restic, rclone and pg_dump; local folder, S3, SFTP
# and SMB destinations). Needs a FRESH stack:
#   docker compose -f deploy/backup-test/docker-compose.yml up -d
#   bash scripts/backup-drill.sh
set -euo pipefail
export PYTHONUTF8=1

BASE="${1:-http://127.0.0.1:8695}"
COMPOSE_FILE_PATH="$(dirname "$0")/../deploy/backup-test/docker-compose.yml"
# MSYS_NO_PATHCONV: stop Git Bash on Windows rewriting /data/... for docker exec.
dc() { MSYS_NO_PATHCONV=1 docker compose -f "$COMPOSE_FILE_PATH" "$@"; }
WORK="$(mktemp -d)"
ADMIN="$WORK/admin.jar"
MEMBER="$WORK/member.jar"
pass=0
T0=$(date +%s)

ok() { pass=$((pass + 1)); printf '  ok  %s  (%ss)\n' "$1" "$(( $(date +%s) - T0 ))"; }
fail() { printf '  FAIL %s\n' "$1"; exit 1; }
jget() { python -c "import sys,json; d=json.load(sys.stdin); print(eval('d'+sys.argv[1]))" "$1"; }
b64() { printf '%s' "$1" | base64 | tr -d '\n'; }
call() { # call <jar> <method> <path> [json] -> body, then a last line with the HTTP code
  local jar=$1 method=$2 path=$3 body=${4:-}
  if [ -n "$body" ]; then
    curl -s -w '\n%{http_code}' -b "$jar" -c "$jar" -X "$method" -H 'content-type: application/json' -d "$body" "$BASE$path"
  else
    curl -s -w '\n%{http_code}' -b "$jar" -c "$jar" -X "$method" "$BASE$path"
  fi
}
api() { # api <jar> <method> <path> [json] -> body; fails on non-2xx
  local out code
  out=$(call "$@"); code=${out##*$'\n'}; out=${out%$'\n'*}
  [[ $code == 2* ]] || { echo "  HTTP $code from $2 $3: $out" >&2; return 1; }
  printf '%s' "$out"
}
expect_error() { # expect_error <code> <text> <label> <call args...>
  local want=$1 text=$2 label=$3; shift 3
  local out code; out=$(call "$@"); code=${out##*$'\n'}; out=${out%$'\n'*}
  [ "$code" = "$want" ] || fail "$label: wanted HTTP $want, got $code ($out)"
  grep -qi -- "$text" <<<"$out" || fail "$label: message should mention '$text': $out"
  ok "$label → $(echo "$out" | jget "['message']")"
}
upload() { # upload <jar> <parent> <name> <file> -> node id
  local len; len=$(wc -c < "$4")
  curl -sf -b "$1" -D - -o /dev/null -X POST -H 'Tus-Resumable: 1.0.0' -H "Upload-Length: $len" \
    -H "Upload-Metadata: filename $(b64 "$3"),parentId $(b64 "$2")" \
    -H 'Content-Type: application/offset+octet-stream' --data-binary @"$4" "$BASE/api/uploads" \
    | tr -d '\r' | awk -F': ' 'tolower($1)=="fennoly-node-id"{print $2}'
}
folder() { api "$1" POST "/api/nodes/$2/folders" "{\"name\":\"$3\"}" | jget "['id']"; }
child() { # child <jar> <folder> <name> -> id (name passed as hex: Windows mangles non-ASCII argv)
  api "$1" GET "/api/nodes/$2/children" | python -c "import sys,json; n=bytes.fromhex(sys.argv[1]).decode(); print(next(c['id'] for c in json.load(sys.stdin) if c['name']==n))" "$(printf '%s' "$3" | od -An -tx1 | tr -d ' \n')"
}
sha() { sha256sum "$1" | cut -d' ' -f1; }
sha_node() { curl -sf -b "$ADMIN" "$BASE/api/nodes/$1/content" | sha256sum | cut -d' ' -f1; }
target_field() { api "$ADMIN" GET "/api/admin/backups/$1" | jget "$2"; }
wait_idle() { # wait_idle <target> [seconds]: until no run is in progress
  local limit=${2:-600} i=0
  while [ "$(target_field "$1" "['running']")" != "None" ]; do
    i=$((i + 2)); [ $i -lt "$limit" ] || fail "run on $1 did not finish in ${limit}s"; sleep 2
  done
}
last_run() { api "$ADMIN" GET "/api/admin/backups/$1/runs?limit=1" | jget "[0]$2"; }
run_backup() { # run_backup <target> <label>: back up now, wait, require success
  api "$ADMIN" POST "/api/admin/backups/$1/run" > /dev/null
  wait_idle "$1"
  local runs; runs=$(api "$ADMIN" GET "/api/admin/backups/$1/runs?limit=5")
  local status; status=$(echo "$runs" | python -c "import sys,json; r=[x for x in json.load(sys.stdin) if x['kind']=='backup'][0]; print(r['status'], r['error'] or '')")
  [[ $status == success* ]] || fail "$2 backup: $status"
  ok "$2: backup succeeded ($(echo "$runs" | python -c "import sys,json; r=[x for x in json.load(sys.stdin) if x['kind']=='backup'][0]; print(r['total_files'], 'files,', r['bytes_added'], 'bytes added')"))"
}

echo "Fennoly backup drill against $BASE"
for i in $(seq 1 60); do curl -sf "$BASE/api/health" > /dev/null && break; sleep 2; done
[ "$(curl -sf "$BASE/api/setup" | jget "['needs_setup']")" = "True" ] || fail "stack must be fresh"

ME=$(api "$ADMIN" POST /api/setup '{"username":"noor","display_name":"Noor","password":"drillpass1","locale":"en"}')
HOME_ID=$(echo "$ME" | jget "['home_id']"); FAMILY_ID=$(echo "$ME" | jget "['family_id']")
api "$ADMIN" POST /api/admin/users '{"username":"aai","display_name":"Aai","password":"drillpass2"}' > /dev/null
api "$MEMBER" POST /api/auth/login '{"username":"aai","password":"drillpass2"}' > /dev/null
AAI_HOME=$(api "$MEMBER" GET /api/me | jget "['home_id']")
LIST=$(api "$ADMIN" GET /api/admin/backups)
[ "$(echo "$LIST" | jget "['engine']['restic']")" = "True" ] && [ "$(echo "$LIST" | jget "['engine']['rclone']")" = "True" ] \
  || fail "engines detected in the image"
ok "image has restic and rclone"

# --- family data -----------------------------------------------------------
head -c 12000000 /dev/urandom > "$WORK/big.bin"
head -c 200000 /dev/urandom > "$WORK/receipt.pdf"
head -c 50000 /dev/urandom > "$WORK/photo.jpg"
printf 'January receipts\n' > "$WORK/jan.txt"
head -c 80000 /dev/urandom > "$WORK/ration.png"
printf 'Aai: buy rice, call Mama\n' > "$WORK/notes.txt"
TAX=$(folder "$ADMIN" "$HOME_ID" "Tax 2026")
REC=$(folder "$ADMIN" "$TAX" "Receipts"); R26=$(folder "$ADMIN" "$REC" "2026")
upload "$ADMIN" "$TAX" "big.bin" "$WORK/big.bin" > /dev/null
upload "$ADMIN" "$TAX" "कर पावती.pdf" "$WORK/receipt.pdf" > /dev/null
upload "$ADMIN" "$TAX" "Photo [1].jpg" "$WORK/photo.jpg" > /dev/null
JAN=$(upload "$ADMIN" "$R26" "jan.txt" "$WORK/jan.txt")
FD=$(folder "$ADMIN" "$FAMILY_ID" "Family Docs")
upload "$ADMIN" "$FD" "ration card.png" "$WORK/ration.png" > /dev/null
upload "$MEMBER" "$AAI_HOME" "Aai notes.txt" "$WORK/notes.txt" > /dev/null
ok "uploaded 6 files: Marathi name, brackets, nested folders, family space, a member's files"

# --- destinations -------------------------------------------------------------
create() { api "$ADMIN" POST /api/admin/backups "$1"; }
OUT=$(create '{"name":"USB","kind":"local","settings":{"path":"/backups/fennoly"},"schedule":"manual"}')
LOCAL=$(echo "$OUT" | jget "['target']['id']"); KEY_LOCAL=$(echo "$OUT" | jget "['recovery_key']")
OUT=$(create '{"name":"Cloud","kind":"s3","settings":{"endpoint":"http://s3:8333","bucket":"fennoly-drill","path":"family"},"secrets":{"access_key":"drillaccess","secret_key":"drillsecret123"},"schedule":"manual"}')
S3=$(echo "$OUT" | jget "['target']['id']")
SFTP=$(create '{"name":"SFTP box","kind":"sftp","encrypted":false,"settings":{"host":"sftp","user":"noor","path":"backups/fennoly"},"secrets":{"password":"sftp-pass"},"schedule":"manual"}' | jget "['target']['id']")
SMB=$(create '{"name":"NAS","kind":"smb","settings":{"host":"samba","share":"backups","path":"fennoly","user":"noor"},"secrets":{"password":"smb-pass"},"schedule":"manual"}' | jget "['target']['id']")
ok "added USB (encrypted), S3 (encrypted), SFTP (plain) and SMB (encrypted)"

for t in "$LOCAL:USB" "$S3:S3" "$SFTP:SFTP" "$SMB:SMB"; do
  api "$ADMIN" POST "/api/admin/backups/${t%%:*}/test" > /dev/null || fail "${t#*:} connection test"
  [ "$(target_field "${t%%:*}" "['initialized']")" = "True" ] || fail "${t#*:} initialized"
  ok "${t#*:}: test connection created the backup store"
done

for t in "$LOCAL:USB" "$S3:S3" "$SFTP:SFTP" "$SMB:SMB"; do run_backup "${t%%:*}" "${t#*:}"; done

RUNS=$(api "$ADMIN" GET "/api/admin/backups/$LOCAL/runs")
echo "$RUNS" | python -c "import sys,json; r=json.load(sys.stdin); assert any(x['kind']=='check' and x['status']=='success' for x in r), r" \
  || fail "first backup is followed by a data check"
ok "first backup was followed by a successful 5% data check"

# The snapshot holds a usable database dump and manifest.
dc exec -T -e RESTIC_PASSWORD="$KEY_LOCAL" fennoly restic -r /backups/fennoly dump latest /data/backup-staging/fennoly.dump \
  > "$WORK/fennoly.dump"
dc exec -T postgres pg_restore --list < "$WORK/fennoly.dump" | grep -q "TABLE public nodes" || fail "dump lists the nodes table"
dc exec -T -e RESTIC_PASSWORD="$KEY_LOCAL" fennoly restic -r /backups/fennoly dump latest /data/backup-staging/manifest.json \
  | jget "['users']" | grep -qx 2 || fail "manifest counts 2 users"
ok "snapshot contains a valid pg_dump ($(wc -c < "$WORK/fennoly.dump") bytes) and manifest"
[ -z "$(dc exec -T fennoly ls -A /data/backup-staging 2>/dev/null)" ] || fail "staging cleaned"
ok "database dump removed from the server after the run"

# Encrypted means encrypted: the plain text is not in the repository files.
if dc exec -T fennoly sh -c 'grep -rl "January receipts" /backups/fennoly' > /dev/null 2>&1; then fail "plain text found in encrypted repo"; fi
ok "file contents are not readable in the encrypted USB backup"

# --- changes, then an incremental backup ---------------------------------------
api "$ADMIN" DELETE "/api/nodes/$JAN" > /dev/null
printf 'added after the first backup\n' > "$WORK/later.txt"
upload "$ADMIN" "$TAX" "later.txt" "$WORK/later.txt" > /dev/null
run_backup "$LOCAL" "USB again"
ADDED=$(last_run "$LOCAL" "['bytes_added']")
[ "$ADDED" -lt 2000000 ] || fail "incremental backup stores only changes (added $ADDED bytes)"
ok "incremental backup added only $ADDED bytes (12 MB file not stored again)"

SNAPS=$(api "$ADMIN" GET "/api/admin/backups/$LOCAL/snapshots")
[ "$(echo "$SNAPS" | jget ".__len__()")" = "2" ] || fail "two snapshots"
OLD=$(echo "$SNAPS" | jget "[1]['id']"); NEW=$(echo "$SNAPS" | jget "[0]['id']")
ok "two snapshots listed, newest first"

# --- browse -------------------------------------------------------------------
ROOT=$(api "$ADMIN" GET "/api/admin/backups/$LOCAL/snapshots/$OLD/ls?path=")
echo "$ROOT" | python -c "import sys,json; e={x['name']:x for x in json.load(sys.stdin)}; assert e['family']['label']=='Family' and e['users']['label']=='Members', e" \
  || fail "root shows Family and Members"
USERS=$(api "$ADMIN" GET "/api/admin/backups/$LOCAL/snapshots/$OLD/ls?path=users")
echo "$USERS" | python -c "import sys,json; e={x['name']:x['label'] for x in json.load(sys.stdin)}; assert e.get('noor')=='Noor' and e.get('aai')=='Aai', e" \
  || fail "members shown by display name: $USERS"
INNER=$(api "$ADMIN" GET "/api/admin/backups/$LOCAL/snapshots/$OLD/ls?path=$(python -c 'import urllib.parse;print(urllib.parse.quote("users/noor/Tax 2026"))')")
echo "$INNER" | python -c "
import sys,json
e={x['name']:x for x in json.load(sys.stdin)}
assert set(e)=={'big.bin','\u0915\u0930 \u092a\u093e\u0935\u0924\u0940.pdf','Photo [1].jpg','Receipts'}, set(e)
assert e['big.bin']['size']==12000000 and e['Receipts']['is_dir']
" || fail "folder listing: $INNER"
ok "browsing shows Family / Members (by name) / folders with sizes"

# --- restore from the OLD snapshot (has jan.txt, lacks later.txt) ---------------
restore() { # restore <target> <snapshot> <json paths> -> waits, requires success
  local run; run=$(api "$ADMIN" POST "/api/admin/backups/$1/snapshots/$2/restore" "{\"paths\":$3}" | jget "['run_id']")
  wait_idle "$1"
  local r; r=$(api "$ADMIN" GET "/api/admin/backups/$1/runs?limit=10" | python -c "import sys,json; r=[x for x in json.load(sys.stdin) if x['id']==sys.argv[1]][0]; print(r['status'], r['error'] or '')" "$run")
  [[ $r == success* ]] || fail "restore: $r"
}
newest_restored() {
  api "$ADMIN" GET "/api/nodes/$HOME_ID/children?sort=modified&order=desc" \
    | python -c "import sys,json; print(next(c['id'] for c in json.load(sys.stdin) if c['name'].startswith('Restored ')))"
}
restore "$LOCAL" "$OLD" '["users/noor/Tax 2026","family/Family Docs/ration card.png","users/aai/Aai notes.txt"]'
RF=$(newest_restored)
RT=$(child "$ADMIN" "$RF" "Tax 2026")
[ "$(sha_node "$(child "$ADMIN" "$RT" "big.bin")")" = "$(sha "$WORK/big.bin")" ] || fail "big.bin identical"
[ "$(sha_node "$(child "$ADMIN" "$RT" "कर पावती.pdf")")" = "$(sha "$WORK/receipt.pdf")" ] || fail "Marathi file identical"
[ "$(sha_node "$(child "$ADMIN" "$RT" "Photo [1].jpg")")" = "$(sha "$WORK/photo.jpg")" ] || fail "bracket file identical"
J=$(child "$ADMIN" "$(child "$ADMIN" "$(child "$ADMIN" "$RT" "Receipts")" "2026")" "jan.txt")
[ "$(sha_node "$J")" = "$(sha "$WORK/jan.txt")" ] || fail "deleted file came back"
[ "$(sha_node "$(child "$ADMIN" "$RF" "ration card.png")")" = "$(sha "$WORK/ration.png")" ] || fail "family file identical"
[ "$(sha_node "$(child "$ADMIN" "$RF" "Aai notes.txt")")" = "$(sha "$WORK/notes.txt")" ] || fail "member file identical"
if child "$ADMIN" "$RT" "later.txt" > /dev/null 2>&1; then fail "old snapshot must not contain later.txt"; fi
ok "restored a nested folder, a family file and a member's file: all 6 byte-identical, deleted file back"
[ "$(api "$ADMIN" GET "/api/nodes/$HOME_ID/children" | python -c "import sys,json; print(sum(c['name']=='Tax 2026' for c in json.load(sys.stdin)))")" = 1 ] \
  || fail "original folder untouched"
ok "the original Tax 2026 folder was left as it is"
[ -z "$(dc exec -T fennoly ls -A /data/tmp/restore 2>/dev/null)" ] || fail "restore temp cleaned"
ok "restore working folder removed"

# Restoring a single file with brackets works through every remote transport.
for t in "$S3:S3" "$SFTP:SFTP" "$SMB:SMB"; do
  SNAP=$(api "$ADMIN" GET "/api/admin/backups/${t%%:*}/snapshots" | jget "[0]['id']")
  restore "${t%%:*}" "$SNAP" '["users/noor/Tax 2026/Photo [1].jpg"]'
  [ "$(sha_node "$(child "$ADMIN" "$(newest_restored)" "Photo [1].jpg")")" = "$(sha "$WORK/photo.jpg")" ] || fail "${t#*:} restore"
  ok "${t#*:}: single-file restore is byte-identical"
done

api "$ADMIN" POST "/api/admin/backups/$LOCAL/snapshots/$NEW/restore" '{"paths":["users/noor/not-there"]}' > /dev/null
wait_idle "$LOCAL"
[ "$(last_run "$LOCAL" "['status']")" = "failed" ] && grep -qi "nothing was found" <<<"$(last_run "$LOCAL" "['error']")" \
  || fail "missing path restore: $(last_run "$LOCAL" "['error']")"
ok "restoring a path that is not in the backup → $(last_run "$LOCAL" "['error']")"

# --- things going wrong ---------------------------------------------------------
bad() { create "$1" | jget "['target']['id']"; }
B=$(bad '{"name":"bad sftp","kind":"sftp","encrypted":false,"settings":{"host":"sftp","user":"noor","path":"backups/x"},"secrets":{"password":"wrong"},"schedule":"manual"}')
expect_error 400 "refused access" "SFTP with a wrong password" "$ADMIN" POST "/api/admin/backups/$B/test"
B=$(bad '{"name":"bad smb","kind":"smb","encrypted":false,"settings":{"host":"samba","share":"backups","user":"noor"},"secrets":{"password":"wrong"},"schedule":"manual"}')
expect_error 400 "refused access" "SMB with a wrong password" "$ADMIN" POST "/api/admin/backups/$B/test"
B=$(bad '{"name":"bad s3","kind":"s3","encrypted":false,"settings":{"endpoint":"http://s3:8333","bucket":"fennoly-drill"},"secrets":{"access_key":"drillaccess","secret_key":"nope"},"schedule":"manual"}')
expect_error 400 "refused access" "S3 with a wrong secret key" "$ADMIN" POST "/api/admin/backups/$B/test"
B=$(bad '{"name":"gone nas","kind":"smb","encrypted":false,"settings":{"host":"nas.invalid","share":"backups","user":"noor"},"secrets":{"password":"x"},"schedule":"manual"}')
expect_error 400 "Cannot reach" "SMB server that does not exist" "$ADMIN" POST "/api/admin/backups/$B/test"
B=$(bad '{"name":"old usb wrong key","kind":"local","settings":{"path":"/backups/fennoly"},"recovery_key":"WRONG-KEY","schedule":"manual"}')
expect_error 400 "recovery key" "existing backup with the wrong recovery key" "$ADMIN" POST "/api/admin/backups/$B/test"
B=$(bad '{"name":"old usb no key","kind":"local","encrypted":false,"settings":{"path":"/backups/fennoly"},"schedule":"manual"}')
expect_error 400 "recovery key" "encrypted backup opened as unencrypted" "$ADMIN" POST "/api/admin/backups/$B/test"

# Reconnecting to an existing backup with its key (as after a reinstall).
B=$(bad "{\"name\":\"old usb\",\"kind\":\"local\",\"settings\":{\"path\":\"/backups/fennoly\"},\"recovery_key\":\"$KEY_LOCAL\",\"schedule\":\"manual\"}")
api "$ADMIN" POST "/api/admin/backups/$B/test" > /dev/null
[ "$(api "$ADMIN" GET "/api/admin/backups/$B/snapshots" | jget ".__len__()")" = "2" ] || fail "reconnected sees snapshots"
ok "reconnecting with the recovery key shows the existing backups"

# A destination that fills up.
TINY=$(bad '{"name":"tiny","kind":"local","encrypted":false,"settings":{"path":"/tiny/repo"},"schedule":"manual"}')
api "$ADMIN" POST "/api/admin/backups/$TINY/run" > /dev/null; wait_idle "$TINY"
[ "$(last_run "$TINY" "['status']")" = "failed" ] || fail "full destination fails"
grep -qi "full" <<<"$(last_run "$TINY" "['error']")" || fail "says it is full: $(last_run "$TINY" "['error']")"
ok "full destination → $(last_run "$TINY" "['error']")"
[ "$(api "$ADMIN" GET /api/admin/backups/status | jget "['last_failed']")" = "True" ] || fail "status reports the failure"
ok "status endpoint reports the failed backup (banner)"
[ -z "$(dc exec -T fennoly ls -A /data/backup-staging 2>/dev/null)" ] || fail "staging cleaned after failure"
ok "database dump removed after a failed run too"

# Busy destination.
api "$ADMIN" POST "/api/admin/backups/$S3/run" > /dev/null
expect_error 409 "already running" "second run while one is going" "$ADMIN" POST "/api/admin/backups/$S3/run"
expect_error 409 "wait" "removing a destination while it runs" "$ADMIN" DELETE "/api/admin/backups/$S3"
wait_idle "$S3"

# Server restart in the middle of a backup.
head -c 400000000 /dev/urandom > "$WORK/huge.bin"
upload "$ADMIN" "$TAX" "huge.bin" "$WORK/huge.bin" > /dev/null
api "$ADMIN" POST "/api/admin/backups/$SMB/run" > /dev/null
# Restart while the dump or the upload is under way (a local SMB container
# takes a 400 MB file in about 3 seconds).
sleep 0.5
[ "$(target_field "$SMB" "['running']")" != "None" ] || fail "backup still running before the restart"
dc restart fennoly > /dev/null
for i in $(seq 1 60); do curl -sf "$BASE/api/health" > /dev/null && break; sleep 2; done
api "$ADMIN" POST /api/auth/login '{"username":"noor","password":"drillpass1"}' > /dev/null
R=$(api "$ADMIN" GET "/api/admin/backups/$SMB/runs?limit=1")
[ "$(echo "$R" | jget "[0]['status']")" = "failed" ] && grep -q "stopped" <<<"$(echo "$R" | jget "[0]['error']")" \
  || fail "interrupted run marked failed: $R"
ok "run cut short by a restart is marked failed"
run_backup "$SMB" "SMB after the restart"

# Scheduled run (time zone Asia/Kolkata in the container).
AT=$(TZ=Asia/Kolkata python -c "import datetime as d; print((d.datetime.now()+d.timedelta(minutes=2)).strftime('%H:%M'))")
api "$ADMIN" PATCH "/api/admin/backups/$SFTP" "{\"schedule\":\"daily $AT\"}" > /dev/null
for i in $(seq 1 120); do
  T=$(api "$ADMIN" GET "/api/admin/backups/$SFTP/runs?limit=1" | jget "[0]['trigger']")
  [ "$T" = "schedule" ] && break; sleep 3
done
[ "$T" = "schedule" ] || fail "scheduled backup did not start by $AT"
wait_idle "$SFTP"
[ "$(last_run "$SFTP" "['status']")" = "success" ] || fail "scheduled run succeeded"
NEXT=$(target_field "$SFTP" "['next_run_at']")
ok "scheduled backup ran at $AT (Asia/Kolkata); next run $NEXT"

echo "All $pass backup drill checks passed in $(( $(date +%s) - T0 ))s."
