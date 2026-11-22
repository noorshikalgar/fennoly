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
