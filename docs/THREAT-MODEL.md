# Threat model

What Fennoly protects, from whom, and how. Read this before putting Fennoly
on the internet. Each claim points to the checks that test it
([SECURITY-CHECKS.md](SECURITY-CHECKS.md)).

## What there is to protect

| Asset | Why it matters |
|---|---|
| **Private files** (each person's *My files*) | ID cards, medical and financial papers. Private even from the family's admin in the app. |
| **The Family space** | The household's shared papers. Private to the family. |
| **Shares and public links** | Deliberate, limited openings. They must never open more than was chosen. |
| **Accounts** | Passwords and sessions. Taking one over means reading and changing that person's files. |
| **Backups** | Encrypted copies of everything, plus the recovery key that opens them. |
| **The server** | The computer Fennoly runs on, usually inside someone's home network. |

## Who might attack

| Who | Can do | Fennoly's answer |
|---|---|---|
| **A stranger on the internet** (only if Fennoly is exposed) | Send any request; guess passwords and link tokens | Everything needs a session or a valid link; sign-in and link passwords are rate limited; HTTPS recommended |
| **Someone with a public link** | Open what it shares | Only what is under the linked folder, only while it is valid, and only with the password if it has one |
| **A curious family member** | Use their own account fully; craft requests by hand | The server checks every request against the owner and the shares of each file, not just what the app shows |
| **A malicious file** (uploaded by anyone with access) | Try to run script when someone opens it | Files that can run script are served as downloads in a sandbox, never as pages of the app |
| **A malicious website** the family visits | Make the browser send requests to Fennoly | SameSite cookies, JSON-only API, no CORS, Content-Security-Policy, no framing |
| **Someone with the server's disk or a backup** | Read stored data | Backups are encrypted; stored secrets are sealed; the files themselves are plain folders, so protect the disk |

## Trust boundaries

```
 phone / browser ──(HTTPS or VPN)──▶ Fennoly server ──▶ PostgreSQL (who owns what, shares, links)
 link visitor    ──────────────────▶   API + checks  ──▶ files as plain folders on disk
                                                     ──▶ restic + rclone ──▶ backups (encrypted)
```

- **The browser is not trusted.** Everything the app hides is also refused by
  the server, which decides access from the database for every request.
- **The server and its disk are trusted.** Files are kept as ordinary folders
  with their real names, by design, so the family can always walk away with
  them. Whoever controls the disk can read them; use disk encryption on the
  server if that matters to you.
- **The admin is trusted with the server, not with private files.** In the
  app, an admin cannot open anyone's *My files*. But an admin manages backups,
  holds their recovery key and can restore them, which includes every file.
  Treat the admin role as "can, in the end, see everything".

## Threats and what stops them

### Getting in

- **Guessing passwords.** Argon2 hashes; after 5 wrong attempts a name is
  locked, for longer and longer. *Checks: brute force.*
- **Finding out who has an account.** Unknown names and wrong passwords get the
  same answer in the same time. *Checks: enumeration, login timing.*
- **Stealing a session.** The cookie is `HttpOnly` and `SameSite`; signing out,
  a password reset or disabling the account ends sessions on the server.
  *Checks: 19 session checks.*

### Someone else's files

- **Private files by id** (read, list, download, thumbnail, zip, rename, move,
  trash, star, share, link, compress, upload into, restore, purge): every
  operation checks access on the server; an id you may not see answers like
  one that does not exist. Search and Recent never show them.
  *Checks: 27 direct-object checks.*
- **Shares doing more than allowed.** A viewer cannot change anything or
  re-share; an editor cannot move the owner's files out of the owner's space
  or re-share them; a share never reveals the owner's other folders.
  *Checks: 15 share checks.*
- **Acting as admin.** Accounts, quotas, backups, recovery keys and restores
  require the admin role on the server. *Checks: 9 privilege checks.*
- **Using up someone's storage**, or escaping your own limit by moving family
  files into a full personal space. *Check: quota.*

### Public links

Link tokens are long and random; a locked link needs its password for every
part of it (listing, files, thumbnails, zips); password guesses are rate
limited; changing the password signs visitors out; expired links and links of
disabled members stop working; nothing outside the linked folder (or in the
trash) can be reached through it. *Checks: 22 link checks.*

### Files that bite

- **Script in an uploaded file.** HTML, SVG, XML and similar files are served
  with `Content-Disposition: attachment` and a `sandbox` Content-Security-Policy,
  so they never run as part of Fennoly. Text is served as `text/plain`.
  *Checks: 8 stored-XSS checks.*
- **Path tricks in names.** `..`, `/`, `\`, NUL bytes, Windows device names
  (`con`), trailing dots and overlong names are refused; an upload named
  `../../escape.txt` lands in its folder as `escape.txt`.
  *Checks: 17 name and path checks.*
- **Malicious PDFs and images in the tools.** Ghostscript runs with `-dSAFER`
  on a copy, inside the container, as an unprivileged user; images are decoded
  by the memory-safe Rust `image` crate.

### Attacks through the browser

- **Cross-site request forgery.** Cookies are `SameSite`, and the API only
  accepts JSON bodies; form-encoded and `text/plain` posts are refused.
- **Reading answers from another site.** No CORS permission is ever given.
- **Framing (clickjacking)** and sniffing: `X-Frame-Options`,
  `frame-ancestors`, `nosniff`, a strict Content-Security-Policy, no server banner.
  *Checks: 10 request-forgery checks, 5 header checks.*

### Backups

Backups are encrypted by restic before they leave the server; the password is
the recovery key, shown to the admin once to print or save. Backup passwords,
cloud keys and recovery keys stored in the database are sealed with
XChaCha20-Poly1305 using a key file kept beside the data, so a database dump
alone reveals no secret. Members cannot list backups, read a recovery key or
start a restore. Restores go into a new folder and never overwrite.

## What Fennoly does not protect against

- **Plain HTTP on a network you do not trust.** Without HTTPS, anyone on the
  same Wi-Fi can read traffic, including passwords and files. Outside your
  home, use HTTPS (Caddy) or a VPN (Tailscale) and set
  `FENNOLY_SECURE_COOKIES=true`.
- **A compromised server.** Files on disk are not encrypted at rest.
- **Losing the recovery key.** Without it, encrypted backups cannot be read,
  by you or anyone. Print it.
- **A compromised phone or browser extension.** Something that controls the
  browser can do whatever the signed-in person can.
- **Denial of service.** A home server can be flooded.

## Reporting

Found a way around any of this? See [SECURITY.md](../SECURITY.md).
