# How uploads work

Fennoly uploads files with its own chunked protocol. The idea comes from
copyparty's up2k, but the design and code here are original.

## The protocol

1. **Prepare.** The browser splits the file into fixed-size chunks and hashes
   each chunk with BLAKE3 in a Web Worker, so the page stays smooth. The chunk
   size is 4 MiB, and it doubles for big files so that no file has more than
   about 2,048 chunks (the maximum is 64 MiB).
2. **Plan.** `POST /api/chunked` sends the file name, size, chunk size and the
   list of chunk hashes. The server creates the upload (it reserves the full
   file size on disk) and answers with the chunks it is **missing**. If the
   same file was already sent to the same folder and interrupted, the server
   finds that attempt and lists only the chunks that are still missing.
3. **Send.** `PUT /api/chunked/{id}/chunks/{n}` for each missing chunk: up to
   6 at a time in total and 4 per file, in any order. The server streams each
   chunk straight into its place in the file while hashing it. **A chunk only
   counts when its hash matches the plan.** A damaged or cut-off chunk stays
   missing and is sent again.
4. **Finish.** `POST /api/chunked/{id}/finish` checks that every chunk has
   arrived, moves the file into its folder and records it. It is safe to repeat
   if the response gets lost.

Because the server accepts only verified chunks, a file cannot be assembled
corrupted, however the connection behaves.

## When the network misbehaves

| Situation | What happens |
| --- | --- |
| A request fails or the connection resets | The chunk goes back in the queue. The file pauses for 1 s, 2 s, 4 s … up to 30 s, then continues. |
| The connection dies silently (requests hang) | After 8 s with no bytes moving, the file shows "Checking the connection". A small `/api/health` check runs every 4 s. If it fails, hung requests are dropped and no new ones start until it succeeds. |
| The network comes back | The next check succeeds and uploading continues within seconds. It does not wait out the pause. |
| The browser reports it is offline | Nothing is sent until the `online` event fires, and then uploading continues at once. |
| A request is dead but the network is fine | It is restarted right away, with no pause. |
| The server restarts (update or crash) | Same as a network outage. The upload state is stored in the database, so it continues where it stopped. |
| The tab is closed or reloaded | "Unfinished uploads" lists the file. Choosing it again continues from where it stopped. The server keeps partial uploads for 3 days. |
| The disk is full | The server answers 507, and the file fails with a clear message instead of retrying forever. |
| The session expires | The file fails with "sign in required". After signing in, Retry continues it. |

## Tested scenarios

Run with `scripts/netsim.py`, a proxy that can throttle, cut and black-hole
connections, plus the real browser client. Every file was checked byte for
byte: the BLAKE3 hash of the original matched the hash the server stored.

| Test | Result |
| --- | --- |
| 1 GiB file, no throttle | About 24 s (about 90 MB/s through two proxies to a debug build). Identical. |
| 300 MB at 40 Mbit/s, 2.5-minute outage halfway | Paused and continued from where it stopped. Identical. |
| 200 MB at 20 Mbit/s, 5-minute outage | Detected in about 5 s, waited quietly, continued within about 1 s of the network returning. Identical. |
| 3 files at once, 30% chance per second of each connection being cut (about 56 cuts) | All three identical. |
| 200 MB, page reloaded at 35%, same file chosen again | Continued from about 35% ("Resumed"). Identical. |
| 150 MB, API server restarted mid-upload | Continued after the restart. Identical. |

Protocol-level checks (`web/scripts/chunked-test.mjs`, run by `scripts/smoke.sh`):
chunks sent in random order and in parallel, flipped-bit, truncated and
oversized chunks refused, resume after interruption, repeated finish, empty
file, invalid plans, other users locked out, and the storage limit enforced.
