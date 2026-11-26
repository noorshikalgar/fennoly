# What the 138 security checks test

`scripts/pentest.sh` behaves like an attacker against a real running Fennoly
(the sandbox, never real data). It signs in as **Alice** (the victim) and
**Mallory** (another family member with bad intentions), and as nobody at
all, and tries to get past each protection in the
[threat model](THREAT-MODEL.md). A pass means the attack was refused the right
way; a failure prints `HOLE`. This version: **138 checks, 0 holes**.

It sits beside `scripts/smoke.sh` (using Fennoly the way a family does),
`scripts/netsim.py` (uploads under a bad network) and `scripts/backup-drill.sh`
(a full backup and restore, see [BACKUPS.md](BACKUPS.md)).

```bash
scripts/reset-test.sh && <start the sandbox server> && scripts/pentest.sh http://127.0.0.1:8687
```

## The attacks, grouped

| Group | Checks | What is tried | What must happen |
|---|---|---|---|
| **Someone else's private files** | 27 | Mallory, by id: reads, lists, downloads, thumbnails and zips Alice's private files (also mixed with his own ids); renames, moves into his home, trashes, stars them; creates folders and uploads (plain and chunked) into her folders; reads, writes, finishes and cancels her uploads; restores and purges her trash; sees her shares and links, shares to himself, makes public links, compresses; looks for her files in search, Recent and trash | Refused every time; her files unchanged and never listed |
| **Shares: viewer and editor** | 15 | A viewer renames, trashes, creates folders, makes links, re-shares, upgrades their own share; an editor moves the owner's files into their own space or the family space, re-shares, links someone's personal files; walking up from a shared folder to the owner's other folders | A viewer only reads; an editor only edits inside the share; nothing above the shared folder is revealed |
| **Names and paths** | 17 | Folder names `..`, `.`, `a/b`, `a\b`, `con`, `x:y`, blanks, a trailing dot; renaming to `..` or with a NUL byte; a very long name; `../../escape.txt` as an upload name; junk and oversized zip id lists; path tricks in the tools' tokens | Refused, or the file stays in its folder with a safe name; nothing written outside the storage folder |
| **Files that run script** | 8 | Uploads `.html`, `.svg`, `.xml`, `.xhtml`, `.mathml`, `.rdf`, `.xsl`, then opens them; a text file | Served as downloads in a sandbox, never run in Fennoly's origin; text as `text/plain` |
| **Stored text** | 1 | A display name with HTML in it | Stored and shown as text |
| **Member as admin** | 9 | A member sets `"role":"admin"` on their profile, lists and creates users, changes their own quota, lists backups, reads a recovery key, starts a restore; reads private fields from the user directory | Refused; still a member; the directory shows names only |
| **Sessions** | 19 | Thirteen kinds of request without signing in; a forged token; the cookie's flags; a cookie replayed after sign-out; a disabled account's session and sign-in; old sessions after a password reset | 401 / refused; `HttpOnly` + `SameSite`; old sessions dead |
| **Brute force and enumeration** | 4 | A 6th wrong password in a minute; the right password while locked; unknown vs known names; the time each takes | Locked; locked even for the right password; same answer; same time |
| **Public links** | 22 | Guessing a token; a locked link's info, listing, files, thumbnails and zip without the password; a forged unlock cookie; wrong and right passwords; reaching outside the linked folder or into the trash; editing or deleting someone's link; old visitors after a password change; an expired link; guessing link passwords quickly; links of a disabled member | Only what the link shares, only with its password, only while valid; guessing is slowed down |
| **Request forgery and odd input** | 10 | Form-encoded and `text/plain` posts to JSON endpoints (CSRF); CORS from another site, with credentials; a malformed id; broken JSON; SQL wildcards and quotes in search; a sort injection; a 3 MB body; error messages | Refused; no CORS; search is only text; sorting ignores injection; errors show no internals |
| **Headers** | 5 | The app page's headers | `nosniff`, `X-Frame-Options`, `Referrer-Policy`, a Content-Security-Policy, no server banner |
| **Quota** | 1 | Escaping a full personal space by moving family files into it | Refused |

## Adding a check

When you fix a security problem, add the attack to `scripts/pentest.sh` first,
watch it print `HOLE`, then fix it and watch it pass.
