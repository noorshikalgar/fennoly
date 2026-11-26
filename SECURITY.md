# Security policy

Fennoly keeps a family's most important papers (ID cards, property papers,
medical records), so security problems matter. Thank you for helping.

## Reporting a vulnerability

**Please do not open a public issue for a security problem.** Report it
privately instead:

1. Go to the repository's **Security** tab and choose
   **Report a vulnerability** (GitHub private vulnerability reporting), or open
   <https://github.com/noorshikalgar/fennoly/security/advisories/new>.
2. Describe what you found, how to reproduce it (the request, the account
   type, the version), and what an attacker could do with it.

What happens next:

| When | What |
|---|---|
| Within 3 days | You get a reply confirming the report was received. |
| Within 10 days | You hear whether it is confirmed, and how serious it looks. |
| As soon as it is fixed | A release with the fix, and an advisory that credits you (unless you prefer not). |

If a fix takes longer, you will be kept informed. Please give us a reasonable
time to fix the problem before you talk about it publicly; 90 days at most.

## What is in scope

- The Fennoly server (`crates/`) and web app (`web/`) in this repository.
- The published Docker image `ghcr.io/noorshikalgar/fennoly`.
- The example deployment files in `deploy/`.

Especially interesting:

- opening, downloading, changing or listing someone else's private files;
- doing more with a share than its permission allows (viewer vs editor);
- reaching outside a public link's folder, or into a locked link without its password;
- a member doing what only an admin may (accounts, quotas, backups, recovery keys);
- an uploaded file that runs script in Fennoly's own origin;
- writing files outside the storage folder (path tricks in names or uploads);
- signing in without the password, or keeping a session after sign-out, a
  password reset or a disabled account.

## Out of scope

- Problems that need someone to already control the server or the admin
  account (an admin manages backups and can restore them, by design).
- Running Fennoly over plain HTTP on an untrusted network (see the threat
  model: use HTTPS or a VPN outside your home).
- Missing rate limits on anything except signing in and link passwords;
  denial of service by flooding a home server.
- Vulnerabilities in Ghostscript, restic or rclone themselves (please report
  those upstream), unless Fennoly uses them unsafely.
- Reports from automated scanners without a demonstrated problem.

## Supported versions

Fixes go into the latest release. Fennoly is young (0.x): please stay on the
newest image.

## How Fennoly is tested

[docs/THREAT-MODEL.md](docs/THREAT-MODEL.md) explains what Fennoly protects,
from whom, and how. [docs/SECURITY-CHECKS.md](docs/SECURITY-CHECKS.md) explains
the 138 attacks `scripts/pentest.sh` tries.
