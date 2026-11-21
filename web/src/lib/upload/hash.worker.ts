/// <reference lib="webworker" />
// Hashes a file chunk by chunk with BLAKE3 (WebAssembly), off the main thread
// so the page stays smooth. Works over plain http too, unlike WebCrypto.

import { createBLAKE3 } from 'hash-wasm';

export interface HashRequest {
	job: number;
	file: File;
	chunkSize: number;
}

export type HashMessage =
	| { job: number; type: 'progress'; done: number; total: number }
	| { job: number; type: 'done'; hashes: string[] }
	| { job: number; type: 'error'; message: string };

const READ = 4 * 1024 * 1024; // read in 4 MiB slices to keep memory low
const queue: HashRequest[] = [];
let busy = false;
const cancelled = new Set<number>();

async function run(req: HashRequest) {
	const hasher = await createBLAKE3();
	const { file, chunkSize, job } = req;
	const count = file.size === 0 ? 0 : Math.ceil(file.size / chunkSize);
	const hashes: string[] = [];
	for (let i = 0; i < count; i++) {
		if (cancelled.has(job)) return;
		hasher.init();
		const end = Math.min(file.size, (i + 1) * chunkSize);
		for (let pos = i * chunkSize; pos < end; pos += READ) {
			const buf = await file.slice(pos, Math.min(end, pos + READ)).arrayBuffer();
			hasher.update(new Uint8Array(buf));
		}
		hashes.push(hasher.digest('hex'));
		postMessage({ job, type: 'progress', done: end, total: file.size } satisfies HashMessage);
	}
	postMessage({ job, type: 'done', hashes } satisfies HashMessage);
}

async function pump() {
	if (busy) return;
	busy = true;
	while (queue.length) {
		const req = queue.shift()!;
		try {
			await run(req);
		} catch (e) {
			postMessage({ job: req.job, type: 'error', message: String(e) } satisfies HashMessage);
		}
	}
	busy = false;
}

self.onmessage = (e: MessageEvent<HashRequest | { cancel: number }>) => {
	if ('cancel' in e.data) {
		cancelled.add(e.data.cancel);
		return;
	}
	queue.push(e.data);
	pump();
};
