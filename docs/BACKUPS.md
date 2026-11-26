# Backups

Fennoly backs up **everything needed to rebuild your drive**: every file in
every space, a dump of the database (accounts, folders, sharing, links) and a
small manifest. Backups run on a schedule, keep a history you can go back to,
and can be encrypted.

Set them up as an admin under **Backups** in the sidebar.

## Where backups can go

| Type | Good for | What you need |
| --- | --- | --- |
| USB drive or folder | A second disk or USB drive on the server | A full path, e.g. `D:\Backups\Fennoly` or `/backups` |
| NAS or Windows share (SMB) | Synology, QNAP, TrueNAS, another PC | Server, share name, user and password |
| SFTP server | Anything you can reach over SSH | Server, user, password, folder |
| S3 cloud storage | Backblaze B2, Wasabi, Cloudflare R2, MinIO, AWS | Endpoint, bucket, access key, secret key |

Google Drive and OneDrive are planned next.

**USB drives with Docker:** mount the drive into the container (see the
commented `/backups` line in `deploy/docker-compose.yml`) and make sure the
folder is writable by user 1000, which Fennoly runs as:
`sudo chown -R 1000 /mnt/usb-backup`. Plug the drive in before the first backup.

A good family setup is **two destinations**: a USB drive or NAS at home
(fast restores) and a cloud bucket (survives fire and theft).

## How it works

* Fennoly uses [restic](https://restic.net) (BSD-2-Clause) to make snapshots and
  [rclone](https://rclone.org) (MIT) to reach SMB and SFTP servers. Both ship in
  the Docker image. Only changed parts of files are stored again, so daily
  backups of a large drive stay small.
* Each run: dump the database with `pg_dump`, snapshot the spaces folder (and
  the trash if you chose it), then thin out old snapshots. The default is 7
  daily, 4 weekly and 12 monthly.
* Once a month, after a backup, Fennoly re-reads a random 5% of the stored
  data to catch a failing disk or a damaged bucket early.
* Backups run one at a time. A run that is cut short by a restart is marked
  failed and the next one starts cleanly.
* If no backup has finished for 3 days, admins see a warning banner.

## Encryption and the recovery key

New destinations are encrypted by default. When you add one, Fennoly shows a
**recovery key** once, for example `7NYS-NFRE-DBR3-BKEG-2PRJ-1BPB-3WZQ-DANV`.

* Fennoly keeps the key itself (sealed with `data/secrets.key`), so backups and
  restores work without you typing it.
* **Save it outside Fennoly** (print it or use a password manager). If the
  server is lost, the key is the only way to open the backups.
* You can show it again under **⋮ → Show recovery key** (asks for your password).
* Unencrypted backups are possible, but anyone who can open the destination
  can read every file.

Passwords and keys for destinations are stored encrypted with
`data/secrets.key`. Back up that file separately, together with the recovery
key, if you want to move Fennoly to a new machine.

## Restoring files

**Backups → Restore files**, pick a snapshot, tick files or folders, and press
**Restore**. They come back in a new `Restored <date>` folder in your files.
Nothing that exists now is changed or overwritten.

## Settings

| Variable | Default | Meaning |
| --- | --- | --- |
| `TZ` | system | Time zone for schedules (`Asia/Kolkata`) |
| `FENNOLY_RESTIC` | `restic` on `PATH` | Path to the restic program |
| `FENNOLY_RCLONE` | `rclone` on `PATH` | Path to the rclone program |
| `FENNOLY_PG_DUMP` | `pg_dump` | Command for the database dump; may include arguments, e.g. `docker exec -i fennoly-dev-postgres-1 pg_dump` |
| `FENNOLY_PG_DUMP_URL` | the server's database URL | Connection string for `pg_dump` when it differs (e.g. inside a container) |

`pg_dump` must be at least as new as the PostgreSQL server (17).

## Without Docker

Install [restic 0.17 or newer](https://github.com/restic/restic/releases),
[rclone](https://rclone.org/downloads/) (for SMB and SFTP) and the PostgreSQL 17
client tools, and make sure they are on `PATH` (or set the variables above).
The Backups page shows a warning while an engine is missing.

## Restoring everything on a new server (by hand, until the setup wizard can)

1. Install Fennoly, but do not create the admin account yet.
2. Restore the latest snapshot:
   `restic -r <repository> restore latest --target /tmp/fennoly-restore`
   (it asks for the recovery key).
3. Load the database: `pg_restore --clean --no-owner -d <database-url> .../backup-staging/fennoly.dump`
4. Copy the `spaces` folder from the restore into the new data folder.
5. Start Fennoly and sign in with your old account.

A guided version of this is planned for the setup wizard.

## Proof that backups restore: the drill

A backup you have never restored is only a hope. `scripts/backup-drill.sh`
runs a full backup and restore against a throw-away Docker stack with real
destinations: a USB-style folder, S3 (SeaweedFS), SFTP and a Windows share
(Samba), using the real restic, rclone and pg_dump inside the Fennoly image.

```bash
docker compose -f deploy/backup-test/docker-compose.yml up -d
bash scripts/backup-drill.sh
docker compose -f deploy/backup-test/docker-compose.yml down -v
```

It uploads files with awkward names (Marathi, brackets, nested folders) into
private and family spaces, backs them up to all four destinations, and then:

- checks the backup is **encrypted** (file contents cannot be found in it) and
  that it holds a valid **database dump**;
- runs a second backup and checks it stored **only the change**, not the 12 MB
  file again;
- deletes files and **restores** a nested folder, a family file and a member's
  file, then compares them **byte for byte**; restores a single file from S3,
  SFTP and SMB the same way; checks the original folder was left untouched;
- tries a **wrong password** for each destination, a server that does not
  exist, the **wrong recovery key**, a **full disk**, two runs at once, and a
  **restart in the middle of a backup**, and checks each gives a clear message
  (and that the backup afterwards works);
- **reconnects** to existing backups with only the recovery key, as you would
  after losing the server.

The output of the latest run (this version):

```text
Fennoly backup drill against http://127.0.0.1:8695
  ok  image has restic and rclone  (3s)
  ok  uploaded 6 files: Marathi name, brackets, nested folders, family space, a member's files  (7s)
  ok  added USB (encrypted), S3 (encrypted), SFTP (plain) and SMB (encrypted)  (9s)
  ok  USB: test connection created the backup store  (12s)
  ok  S3: test connection created the backup store  (16s)
  ok  SFTP: test connection created the backup store  (25s)
  ok  SMB: test connection created the backup store  (28s)
  ok  USB: backup succeeded (8 files, 12375802 bytes added)  (32s)
  ok  S3: backup succeeded (8 files, 12375997 bytes added)  (35s)
  ok  SFTP: backup succeeded (8 files, 12376071 bytes added)  (49s)
  ok  SMB: backup succeeded (8 files, 12376149 bytes added)  (54s)
  ok  first backup was followed by a successful 5% data check  (55s)
  ok  snapshot contains a valid pg_dump (37909 bytes) and manifest  (58s)
  ok  database dump removed from the server after the run  (58s)
  ok  file contents are not readable in the encrypted USB backup  (58s)
  ok  USB again: backup succeeded (8 files, 45409 bytes added)  (62s)
  ok  incremental backup added only 45409 bytes (12 MB file not stored again)  (62s)
  ok  two snapshots listed, newest first  (63s)
  ok  browsing shows Family / Members (by name) / folders with sizes  (66s)
  ok  restored a nested folder, a family file and a member's file: all 6 byte-identical, deleted file back  (72s)
  ok  the original Tax 2026 folder was left as it is  (72s)
  ok  restore working folder removed  (73s)
  ok  S3: single-file restore is byte-identical  (77s)
  ok  SFTP: single-file restore is byte-identical  (86s)
  ok  SMB: single-file restore is byte-identical  (90s)
  ok  restoring a path that is not in the backup → nothing was found in the backup at the chosen place  (93s)
  ok  SFTP with a wrong password → The destination refused access. Check the user name, password or keys.  (94s)
  ok  SMB with a wrong password → The destination refused access. Check the user name, password or keys.  (94s)
  ok  S3 with a wrong secret key → The destination refused access. Check the user name, password or keys.  (95s)
  ok  SMB server that does not exist → Cannot reach the backup destination. Check that it is switched on and connected.  (96s)
  ok  existing backup with the wrong recovery key → The recovery key does not match this backup destination.  (96s)
  ok  encrypted backup opened as unencrypted → The recovery key does not match this backup destination.  (97s)
  ok  reconnecting with the recovery key shows the existing backups  (99s)
  ok  full destination → The backup destination is full.  (104s)
  ok  status endpoint reports the failed backup (banner)  (105s)
  ok  database dump removed after a failed run too  (105s)
  ok  second run while one is going → a backup to this destination is already running  (105s)
  ok  removing a destination while it runs → wait for the current run to finish  (106s)
  ok  run cut short by a restart is marked failed  (113s)
  ok  SMB after the restart: backup succeeded (18 files, 400072084 bytes added)  (118s)
  ok  scheduled backup ran at 13:19 (Asia/Kolkata); next run 2026-11-27T07:49:00Z  (243s)
All 41 backup drill checks passed in 243s.
```

