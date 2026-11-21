"""Network simulator for testing uploads over bad connections.

A TCP proxy that sits between the browser (or the Vite dev server) and the
Fennoly API, and can be told to misbehave:

    python scripts/netsim.py --listen 8690 --target 127.0.0.1:8687 --control 8691

Control it over HTTP (e.g. with curl):

    /rate?kbps=512        limit upload speed (client -> server) to 512 kbit/s; 0 = unlimited
    /offline?seconds=180  cut every connection and refuse new ones for 3 minutes
                          (seconds=0: until /online)
    /online               end an offline period now
    /flaky?p=0.05         each second, cut each open connection with probability 5%
    /status               current settings and counters

Only for development; it is not part of Fennoly.
"""

import argparse
import asyncio
import json
import random
import time
from urllib.parse import parse_qs, urlparse

state = {
    "kbps": 0.0,
    "offline_until": 0.0,  # 0 = online; inf = until /online
    "flaky": 0.0,
    "bytes_up": 0,
    "connections": 0,
    "cut": 0,
}
open_conns: set[asyncio.StreamWriter] = set()


class Bucket:
    """Token bucket shared by all connections (one 'line' to the internet)."""

    def __init__(self):
        self.tokens = 0.0
        self.last = time.monotonic()

    async def take(self, n: int):
        while True:
            rate = state["kbps"] * 1000 / 8  # bytes per second
            if rate <= 0:
                return
            now = time.monotonic()
            self.tokens = min(rate, self.tokens + (now - self.last) * rate)
            self.last = now
            if self.tokens >= n:
                self.tokens -= n
                return
            await asyncio.sleep(max(0.005, (n - self.tokens) / rate))


bucket = Bucket()


def offline() -> bool:
    return time.monotonic() < state["offline_until"]


def cut_all():
    for w in list(open_conns):
        try:
            w.transport.abort()
        except Exception:
            pass
        state["cut"] += 1
    open_conns.clear()


async def pipe(reader, writer, upstream: bool):
    try:
        while True:
            data = await reader.read(16 * 1024)
            if not data:
                break
            if offline():
                break
            if upstream:
                await bucket.take(len(data))
                state["bytes_up"] += len(data)
            writer.write(data)
            await writer.drain()
    except Exception:
        pass
    finally:
        try:
            writer.transport.abort()
        except Exception:
            pass


async def handle(client_r, client_w, target):
    if offline():
        client_w.transport.abort()
        return
    try:
        server_r, server_w = await asyncio.open_connection(*target)
    except OSError:
        client_w.transport.abort()
        return
    state["connections"] += 1
    open_conns.update({client_w, server_w})
    await asyncio.gather(pipe(client_r, server_w, True), pipe(server_r, client_w, False))
    open_conns.discard(client_w)
    open_conns.discard(server_w)


async def flaky_loop():
    while True:
        await asyncio.sleep(1)
        if state["flaky"] > 0:
            for w in list(open_conns):
                if random.random() < state["flaky"]:
                    w.transport.abort()
                    open_conns.discard(w)
                    state["cut"] += 1


async def control(reader, writer):
    line = (await reader.readline()).decode(errors="replace")
    while (await reader.readline()) not in (b"\r\n", b"\n", b""):
        pass
    url = urlparse(line.split(" ")[1] if " " in line else "/")
    q = {k: v[0] for k, v in parse_qs(url.query).items()}
    if url.path == "/rate":
        state["kbps"] = float(q.get("kbps", 0))
    elif url.path == "/offline":
        secs = float(q.get("seconds", 0))
        state["offline_until"] = float("inf") if secs == 0 else time.monotonic() + secs
        cut_all()
    elif url.path == "/online":
        state["offline_until"] = 0.0
    elif url.path == "/flaky":
        state["flaky"] = float(q.get("p", 0))
    body = json.dumps({**state, "offline": offline(), "offline_until": None}).encode()
    writer.write(b"HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: %d\r\n\r\n" % len(body) + body)
    await writer.drain()
    writer.close()


async def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--listen", type=int, default=8690)
    ap.add_argument("--target", default="127.0.0.1:8687")
    ap.add_argument("--control", type=int, default=8691)
    args = ap.parse_args()
    host, port = args.target.split(":")
    target = (host, int(port))
    proxy = await asyncio.start_server(lambda r, w: handle(r, w, target), "127.0.0.1", args.listen, backlog=2048)
    ctl = await asyncio.start_server(control, "127.0.0.1", args.control)
    print(f"netsim: 127.0.0.1:{args.listen} -> {args.target}, control on :{args.control}", flush=True)
    asyncio.create_task(flaky_loop())
    async with proxy, ctl:
        await asyncio.gather(proxy.serve_forever(), ctl.serve_forever())


if __name__ == "__main__":
    asyncio.run(main())
