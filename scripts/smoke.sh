#!/usr/bin/env bash
# End-to-end smoke test against a running Fennoly server with an EMPTY database.
#   scripts/smoke.sh [base-url]
# Exercises setup, folders, plain + parallel tus uploads, download with range,
# thumbnails, sharing, trash/restore, search, zip and admin user management.
set -euo pipefail

BASE="${1:-http://127.0.0.1:8686}"
WORK="$(mktemp -d)"
ADMIN="$WORK/admin.jar"
MEMBER="$WORK/member.jar"
pass=0

ok() { pass=$((pass + 1)); printf '  ok  %s\n' "$1"; }
fail() { printf '  FAIL %s\n' "$1"; exit 1; }
jget() { python -c "import sys,json; d=json.load(sys.stdin); print(eval('d'+sys.argv[1]))" "$1"; }
api() { # api <jar> <method> <path> [json] -> prints the body; fails loudly on non-2xx
  local jar=$1 method=$2 path=$3 body=${4:-} out code
  if [ -n "$body" ]; then
    out=$(curl -s -w '\n%{http_code}' -b "$jar" -c "$jar" -X "$method" -H 'content-type: application/json' -d "$body" "$BASE$path")
  else
    out=$(curl -s -w '\n%{http_code}' -b "$jar" -c "$jar" -X "$method" "$BASE$path")
  fi
  code=${out##*$'\n'}
  out=${out%$'\n'*}
  if [[ $code != 2* ]]; then
    echo "  HTTP $code from $method $path: $out" >&2
    return 1
  fi
  printf '%s' "$out"
}
b64() { printf '%s' "$1" | base64 | tr -d '\n'; }

echo "Fennoly smoke test against $BASE"

[ "$(curl -sf "$BASE/api/setup" | jget "['needs_setup']")" = "True" ] || fail "server must be fresh"
ok "fresh server needs setup"

ME=$(api "$ADMIN" POST /api/setup '{"username":"Noor","display_name":"Noor","password":"supersecret1","locale":"en"}')
HOME_ID=$(echo "$ME" | jget "['home_id']")
FAMILY_ID=$(echo "$ME" | jget "['family_id']")
[ "$(echo "$ME" | jget "['role']")" = "admin" ] || fail "setup creates admin"
ok "setup created admin noor with home and family spaces"

curl -s -o /dev/null -w '%{http_code}' -X POST -H 'content-type: application/json' \
  -d '{"username":"x2","display_name":"x","password":"supersecret1"}' "$BASE/api/setup" | grep -q 409 || fail "second setup refused"
ok "second setup is refused"

FOLDER=$(api "$ADMIN" POST "/api/nodes/$HOME_ID/folders" '{"name":"Tax 2026"}' | jget "['id']")
code=$(curl -s -o /dev/null -w '%{http_code}' -b "$ADMIN" -X POST -H 'content-type: application/json' -d '{"name":"tax 2026"}' "$BASE/api/nodes/$HOME_ID/folders")
[ "$code" = "409" ] || fail "duplicate folder name gives 409 (got $code)"
ok "created folder; duplicate name (any case) refused"

# Plain tus upload (creation-with-upload).
head -c 300000 /dev/urandom > "$WORK/a.bin"
META="filename $(b64 'आधार card.bin'),parentId $(b64 "$FOLDER")"
NODE_A=$(curl -sf -b "$ADMIN" -D - -o /dev/null -X POST \
  -H 'Tus-Resumable: 1.0.0' -H "Upload-Length: 300000" -H "Upload-Metadata: $META" \
  -H 'Content-Type: application/offset+octet-stream' --data-binary @"$WORK/a.bin" \
  "$BASE/api/uploads" | tr -d '\r' | awk -F': ' 'tolower($1)=="fennoly-node-id"{print $2}')
[ -n "$NODE_A" ] || fail "plain upload returns node id"
curl -sf -b "$ADMIN" "$BASE/api/nodes/$NODE_A/content" -o "$WORK/a.out"
cmp -s "$WORK/a.bin" "$WORK/a.out" || fail "downloaded bytes match"
ok "plain upload with a Hindi name, bytes match on download"

# Parallel upload: two partial uploads, then a final concatenation.
head -c 200000 /dev/urandom > "$WORK/p1"; head -c 150000 /dev/urandom > "$WORK/p2"
cat "$WORK/p1" "$WORK/p2" > "$WORK/whole"
part() {
  local file=$1 len; len=$(wc -c < "$file")
  local loc; loc=$(curl -sf -b "$ADMIN" -D - -o /dev/null -X POST -H 'Tus-Resumable: 1.0.0' \
    -H 'Upload-Concat: partial' -H "Upload-Length: $len" "$BASE/api/uploads" \
    | tr -d '\r' | awk -F': ' 'tolower($1)=="location"{print $2}')
  # send in two chunks to exercise PATCH offsets
  head -c 100000 "$file" > "$file.c1"; tail -c +100001 "$file" > "$file.c2"
  curl -sf -b "$ADMIN" -X PATCH -H 'Tus-Resumable: 1.0.0' -H 'Upload-Offset: 0' \
    -H 'Content-Type: application/offset+octet-stream' --data-binary @"$file.c1" "$BASE$loc" > /dev/null
  local off; off=$(curl -sf -b "$ADMIN" -I -H 'Tus-Resumable: 1.0.0' "$BASE$loc" | tr -d '\r' | awk -F': ' 'tolower($1)=="upload-offset"{print $2}')
  [ "$off" = "100000" ] || fail "HEAD reports resume offset (got $off)"
  curl -sf -b "$ADMIN" -X PATCH -H 'Tus-Resumable: 1.0.0' -H 'Upload-Offset: 100000' \
    -H 'Content-Type: application/offset+octet-stream' --data-binary @"$file.c2" "$BASE$loc" > /dev/null
  echo "$loc"
}
L1=$(part "$WORK/p1"); L2=$(part "$WORK/p2")
META2="filename $(b64 'joined.bin'),parentId $(b64 "$FOLDER")"
NODE_P=$(curl -sf -b "$ADMIN" -D - -o /dev/null -X POST -H 'Tus-Resumable: 1.0.0' \
  -H "Upload-Concat: final;$L1 $L2" -H "Upload-Metadata: $META2" "$BASE/api/uploads" \
  | tr -d '\r' | awk -F': ' 'tolower($1)=="fennoly-node-id"{print $2}')
curl -sf -b "$ADMIN" "$BASE/api/nodes/$NODE_P/content" -o "$WORK/joined.out"
cmp -s "$WORK/whole" "$WORK/joined.out" || fail "parallel upload joined correctly"
ok "parallel upload: 2 parts, chunked PATCH, HEAD resume offset, joined file matches"

range=$(curl -sf -b "$ADMIN" -H 'Range: bytes=10-19' "$BASE/api/nodes/$NODE_P/content" | wc -c)
[ "$range" -eq 10 ] || fail "range request returns 10 bytes (got $range)"
ok "range request returns exactly the requested bytes"

# Image upload + thumbnail.
python - "$WORK/photo.png" <<'PY'
import sys, zlib, struct
w, h = 64, 48
raw = b''.join(b'\x00' + bytes(v for x in range(w) for v in (x * 4 % 256, y * 5 % 256, 120)) for y in range(h))
def chunk(t, d): return struct.pack('>I', len(d)) + t + d + struct.pack('>I', zlib.crc32(t + d) & 0xffffffff)
png = b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', w, h, 8, 2, 0, 0, 0)) + chunk(b'IDAT', zlib.compress(raw)) + chunk(b'IEND', b'')
open(sys.argv[1], 'wb').write(png)
PY
len=$(wc -c < "$WORK/photo.png")
META3="filename $(b64 'photo.png'),parentId $(b64 "$FOLDER")"
NODE_IMG=$(curl -sf -b "$ADMIN" -D - -o /dev/null -X POST -H 'Tus-Resumable: 1.0.0' -H "Upload-Length: $len" \
  -H "Upload-Metadata: $META3" -H 'Content-Type: application/offset+octet-stream' --data-binary @"$WORK/photo.png" \
  "$BASE/api/uploads" | tr -d '\r' | awk -F': ' 'tolower($1)=="fennoly-node-id"{print $2}')
ctype=$(curl -sf -b "$ADMIN" -o /dev/null -w '%{content_type}' "$BASE/api/nodes/$NODE_IMG/thumbnail")
[ "$ctype" = "image/jpeg" ] || fail "thumbnail is a jpeg (got $ctype)"
ok "image thumbnail generated"

PREV=$(api "$ADMIN" POST /api/tools/preview "{\"node_id\":\"$NODE_IMG\",\"tool\":\"image\",\"quality\":40}")
TOKEN=$(echo "$PREV" | jget "['token']")
COPY=$(api "$ADMIN" POST /api/tools/apply "{\"node_id\":\"$NODE_IMG\",\"token\":\"$TOKEN\",\"mode\":\"copy\"}" | jget "['name']")
[ "$COPY" = "photo (compressed).jpg" ] || fail "compressed copy name (got $COPY)"
ok "image compress preview ($(echo "$PREV" | jget "['original_size']") -> $(echo "$PREV" | jget "['result_size']") bytes) and keep as copy"

N=$(api "$ADMIN" GET "/api/nodes/$FOLDER/children" | jget ".__len__()")
[ "$N" = "4" ] || fail "folder has 4 files (got $N)"
ok "folder lists 4 files"

# Admin creates a member; sharing.
MEMBER_ID=$(api "$ADMIN" POST /api/admin/users '{"username":"aai","display_name":"Aai","password":"supersecret2"}' | jget "['id']")
api "$MEMBER" POST /api/auth/login '{"username":"AAI","password":"supersecret2"}' > /dev/null
code=$(curl -s -o /dev/null -w '%{http_code}' -b "$MEMBER" "$BASE/api/nodes/$FOLDER")
[ "$code" = "404" ] || fail "member cannot see admin's folder before sharing (got $code)"
api "$ADMIN" POST "/api/nodes/$FOLDER/shares" "{\"user_id\":\"$MEMBER_ID\",\"permission\":\"viewer\"}" > /dev/null
[ "$(api "$MEMBER" GET /api/shared | jget "[0]['name']")" = "Tax 2026" ] || fail "shared item appears for member"
crumbs=$(api "$MEMBER" GET "/api/nodes/$NODE_A" | jget "['crumbs'].__len__()")
[ "$crumbs" = "2" ] || fail "member sees trail from the shared folder only (got $crumbs)"
code=$(curl -s -o /dev/null -w '%{http_code}' -b "$MEMBER" -X DELETE "$BASE/api/nodes/$NODE_A")
[ "$code" = "403" ] || fail "viewer cannot delete (got $code)"
ok "member login (case-insensitive), share as viewer, breadcrumbs hide owner's folders, viewer cannot delete"

FAM=$(api "$MEMBER" POST "/api/nodes/$FAMILY_ID/folders" '{"name":"Insurance"}' | jget "['permission']")
[ "$FAM" = "editor" ] || fail "member can write in family space"
ok "member can create folders in the Family space"

# Search, star, recent.
[ "$(api "$ADMIN" GET "/api/search?q=%E0%A4%86%E0%A4%A7%E0%A4%BE%E0%A4%B0" | jget ".__len__()")" = "1" ] || fail "search finds Hindi name"
api "$ADMIN" PUT "/api/nodes/$NODE_P/star" > /dev/null
[ "$(api "$ADMIN" GET /api/starred | jget "[0]['id']")" = "$NODE_P" ] || fail "starred list"
ok "search by Hindi word, star and starred list"

# Public links: password, view-only, then downloads allowed.
ANON="$WORK/anon.jar"
LINK=$(api "$ADMIN" POST "/api/nodes/$FOLDER/links" '{"allow_download":false,"password":"open1234"}')
TOKEN_L=$(echo "$LINK" | jget "['token']"); LINK_ID=$(echo "$LINK" | jget "['id']")
code=$(curl -s -o /dev/null -w '%{http_code}' -c "$ANON" -b "$ANON" "$BASE/api/public/$TOKEN_L")
[ "$code" = "401" ] || fail "password link needs unlocking (got $code)"
code=$(curl -s -o /dev/null -w '%{http_code}' -c "$ANON" -b "$ANON" -H 'content-type: application/json' -d '{"password":"nope"}' "$BASE/api/public/$TOKEN_L/unlock")
[ "$code" = "400" ] || fail "wrong link password refused (got $code)"
curl -sf -c "$ANON" -b "$ANON" -H 'content-type: application/json' -d '{"password":"open1234"}' "$BASE/api/public/$TOKEN_L/unlock" > /dev/null
[ "$(curl -sf -b "$ANON" "$BASE/api/public/$TOKEN_L" | jget "['root']['name']")" = "Tax 2026" ] || fail "unlocked link shows the folder"
[ "$(curl -sf -b "$ANON" "$BASE/api/public/$TOKEN_L/folder/$FOLDER" | jget "['children'].__len__()")" = "4" ] || fail "link lists folder contents"
code=$(curl -s -o /dev/null -w '%{http_code}' -b "$ANON" "$BASE/api/public/$TOKEN_L/content/$NODE_A")
[ "$code" = "200" ] || fail "view-only link can preview (got $code)"
code=$(curl -s -o /dev/null -w '%{http_code}' -b "$ANON" "$BASE/api/public/$TOKEN_L/content/$NODE_A?download=1")
[ "$code" = "403" ] || fail "view-only link cannot download (got $code)"
code=$(curl -s -o /dev/null -w '%{http_code}' -b "$ANON" "$BASE/api/public/$TOKEN_L/content/$HOME_ID")
[ "$code" = "404" ] || fail "link cannot reach outside the shared folder (got $code)"
api "$ADMIN" PATCH "/api/links/$LINK_ID" '{"allow_download":true}' > /dev/null
curl -sf -b "$ANON" "$BASE/api/public/$TOKEN_L/content/$NODE_A?download=1" -o "$WORK/pub.out"
cmp -s "$WORK/a.bin" "$WORK/pub.out" || fail "public download matches"
code=$(curl -s -o /dev/null -w '%{http_code}' -X PATCH -b "$ADMIN" -H 'content-type: application/json' -d '{"expires_at":"2001-01-01T00:00:00Z"}' "$BASE/api/links/$LINK_ID")
[ "$code" = "400" ] || fail "expiry in the past refused (got $code)"
SBM=$(api "$ADMIN" GET /api/shared-by-me)
[ "$(echo "$SBM" | jget "[0]['shares'].__len__()"),$(echo "$SBM" | jget "[0]['links'][0]['download_count']")" = "1,1" ] || fail "shared-by-me lists the share and the link with its download"
api "$ADMIN" DELETE "/api/nodes/$FOLDER/sharing" > /dev/null
[ "$(api "$ADMIN" GET /api/shared-by-me | jget ".__len__()")" = "0" ] || fail "stop sharing removes everything"
code=$(curl -s -o /dev/null -w '%{http_code}' -b "$ANON" "$BASE/api/public/$TOKEN_L")
[ "$code" = "404" ] || fail "deleted link is gone (got $code)"
ok "public link: password, view-only, stays inside its folder, download toggle, shared-by-me, stop sharing"

# Rename + move + trash + restore.
SUB=$(api "$ADMIN" POST "/api/nodes/$FOLDER/folders" '{"name":"Receipts"}' | jget "['id']")
api "$ADMIN" PATCH "/api/nodes/$NODE_P" "{\"name\":\"receipt.bin\",\"parent_id\":\"$SUB\"}" > /dev/null
curl -sf -b "$ADMIN" "$BASE/api/nodes/$NODE_P/content" -o "$WORK/moved.out"; cmp -s "$WORK/whole" "$WORK/moved.out" || fail "moved file readable"
code=$(curl -s -o /dev/null -w '%{http_code}' -b "$ADMIN" -X PATCH -H 'content-type: application/json' -d "{\"parent_id\":\"$SUB\"}" "$BASE/api/nodes/$FOLDER")
[ "$code" = "400" ] || fail "folder cannot move into its own child (got $code)"
api "$ADMIN" DELETE "/api/nodes/$SUB" > /dev/null
code=$(curl -s -o /dev/null -w '%{http_code}' -b "$ADMIN" "$BASE/api/nodes/$NODE_P")
[ "$code" = "404" ] || fail "file inside trashed folder is hidden (got $code)"
[ "$(api "$ADMIN" GET /api/trash | jget ".__len__()")" = "1" ] || fail "trash lists folder"
api "$ADMIN" POST "/api/trash/$SUB/restore" > /dev/null
curl -sf -b "$ADMIN" "$BASE/api/nodes/$NODE_P/content" -o "$WORK/restored.out"; cmp -s "$WORK/whole" "$WORK/restored.out" || fail "restored file readable"
ok "rename+move, no move into own child, trash hides contents, restore brings them back"

api "$ADMIN" DELETE "/api/nodes/$NODE_A" > /dev/null
api "$ADMIN" DELETE "/api/trash/$NODE_A" > /dev/null
[ "$(api "$ADMIN" GET /api/trash | jget ".__len__()")" = "0" ] || fail "purge empties trash"
ok "delete forever"

ZIPN=$(curl -sf -b "$ADMIN" "$BASE/api/download/zip?ids=$FOLDER" -o "$WORK/f.zip" -w '%{size_download}')
python -c "import zipfile,sys; n=zipfile.ZipFile(sys.argv[1]).namelist(); assert 'Tax 2026/Receipts/receipt.bin' in n, n" "$WORK/f.zip" || fail "zip contents"
ok "folder zip download ($ZIPN bytes) has the right structure"

code=$(curl -s -o /dev/null -w '%{http_code}' -b "$MEMBER" "$BASE/api/admin/users")
[ "$code" = "403" ] || fail "member cannot use admin api (got $code)"
api "$ADMIN" PATCH "/api/admin/users/$MEMBER_ID" '{"disabled":true}' > /dev/null
code=$(curl -s -o /dev/null -w '%{http_code}' -b "$MEMBER" "$BASE/api/me")
[ "$code" = "401" ] || fail "disabled member is signed out (got $code)"
ok "admin api is admin-only; disabling a user ends their sessions"

rm -rf "$WORK"
echo "All $pass checks passed."

# Chunked upload protocol (needs web/node_modules for hash-wasm).
(cd "$(dirname "$0")/../web" && node scripts/chunked-test.mjs "$BASE" noor supersecret1)
