// Pure helpers for the chunked upload protocol (no browser APIs), so they can
// be reasoned about and tested on their own.

const MIB = 1024 * 1024;

/**
 * Chunk size for a file: 4 MiB for typical documents, doubling for big files
 * so a file never has more than ~2,048 chunks. Capped at 64 MiB (server limit).
 */
export function chunkSizeFor(size: number): number {
	let chunk = 4 * MIB;
	while (size / chunk > 2048 && chunk < 64 * MIB) chunk *= 2;
	return chunk;
}

export function chunkCount(size: number, chunk: number): number {
	return size === 0 ? 0 : Math.ceil(size / chunk);
}

export function chunkRange(index: number, size: number, chunk: number): [number, number] {
	const start = index * chunk;
	return [start, Math.min(size, start + chunk)];
}

/** Retry delay after `failures` consecutive failures: 1s, 2s, 4s … capped at 30s, with jitter. */
export function backoff(failures: number): number {
	const base = Math.min(30_000, 1000 * 2 ** Math.max(0, failures - 1));
	return Math.round(base * (0.8 + Math.random() * 0.4));
}

/** Errors worth retrying: no response, timeouts, rate limits and server hiccups. */
export function isTransient(status: number): boolean {
	// 507: the server's disk is full; 501: not supported. Retrying will not help.
	if (status === 507 || status === 501) return false;
	return status === 0 || status === 408 || status === 429 || status >= 500;
}
