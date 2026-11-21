// Protocol test for chunked uploads, run against a live server:
//   node web/scripts/chunked-test.mjs http://127.0.0.1:8687 <admin-user> <admin-password>
// Covers parallel out-of-order chunks, corrupted and truncated chunks,
// resuming an interrupted upload, idempotent finish, empty files,
// permissions, plan validation and quota.

import { createHash, randomBytes } from 'node:crypto';
import { blake3 } from 'hash-wasm';

const [, , BASE = 'http://127.0.0.1:8687', ADMIN = 'noor', PASSWORD = 'supersecret1'] = process.argv;
const MIB = 1024 * 1024;
let passed = 0;

const ok = (msg) => {
	passed++;
	console.log(`  ok  ${msg}`);
};
function assert(cond, msg) {
	if (!cond) {
		console.log(`  FAIL ${msg}`);
		process.exit(1);
	}
}

async function login(username, password) {
	const res = await fetch(`${BASE}/api/auth/login`, {
		method: 'POST',
		headers: { 'content-type': 'application/json' },
		body: JSON.stringify({ username, password })
	});
	assert(res.ok, `login ${username} (${res.status})`);
	const cookie = res.headers.get('set-cookie').split(';')[0];
	const me = await res.json();
	return { cookie, me };
}

function client(cookie) {
	return async (method, path, body, raw = false) => {
		const res = await fetch(`${BASE}/api${path}`, {
			method,
			headers: raw ? { cookie } : { cookie, 'content-type': 'application/json' },
			body: raw ? body : body === undefined ? undefined : JSON.stringify(body)
		});
		const text = await res.text();
		let json = null;
		try {
			json = JSON.parse(text);
		} catch {
			/* not json */
		}
		return { status: res.status, json, text };
	};
}

async function planFor(buf, chunkSize) {
	const hashes = [];
	for (let i = 0; i * chunkSize < buf.length; i++) hashes.push(await blake3(buf.subarray(i * chunkSize, (i + 1) * chunkSize)));
	return hashes;
}

const sha = (b) => createHash('sha256').update(b).digest('hex');
const chunk = (buf, i, size) => buf.subarray(i * size, Math.min(buf.length, (i + 1) * size));
const shuffle = (a) => a.map((v) => [Math.random(), v]).sort((x, y) => x[0] - y[0]).map(([, v]) => v);

console.log(`Chunked upload protocol test against ${BASE}`);
const { cookie, me } = await login(ADMIN, PASSWORD);
const call = client(cookie);
const folder = (await call('POST', `/nodes/${me.home_id}/folders`, { name: `chunk-test-${Date.now()}` })).json;
assert(folder?.id, 'create test folder');

// 1. A 25 MiB + 123 B file in 4 MiB chunks, sent in random order, 6 at a time.
{
	const buf = randomBytes(25 * MIB + 123);
	const size = 4 * MIB;
	const hashes = await planFor(buf, size);
	const plan = (await call('POST', '/chunked', { parent_id: folder.id, filename: 'big.bin', size: buf.length, chunk_size: size, chunk_hashes: hashes })).json;
	assert(plan.missing.length === 7 && !plan.resumed, 'plan for new file lists every chunk');
	const order = shuffle(plan.missing);
	while (order.length) {
		const batch = order.splice(0, 6);
		const results = await Promise.all(batch.map((i) => call('PUT', `/chunked/${plan.id}/chunks/${i}`, chunk(buf, i, size), true)));
		assert(results.every((r) => r.status === 200), 'parallel chunks accepted');
	}
	const fin = await call('POST', `/chunked/${plan.id}/finish`);
	assert(fin.status === 200, `finish (${fin.status} ${fin.text})`);
	const dl = Buffer.from(await (await fetch(`${BASE}/api/nodes/${fin.json.node_id}/content`, { headers: { cookie } })).arrayBuffer());
	assert(sha(dl) === sha(buf), 'downloaded bytes match the original');
	ok('25 MiB file, 7 chunks in random order, 6 in parallel: bytes identical after download');

	const again = await call('POST', `/chunked/${plan.id}/finish`);
	assert(again.status === 200 && again.json.node_id === fin.json.node_id, 'second finish returns same file');
	ok('finish is safe to repeat (lost response): same file, no duplicate');
}

// 2. Damaged and truncated chunks are refused and stay missing.
{
	const buf = randomBytes(3 * MIB);
	const size = MIB;
	const hashes = await planFor(buf, size);
	const plan = (await call('POST', '/chunked', { parent_id: folder.id, filename: 'damaged.bin', size: buf.length, chunk_size: size, chunk_hashes: hashes })).json;
	const bad = Buffer.from(chunk(buf, 1, size));
	bad[12345] ^= 0xff;
	let r = await call('PUT', `/chunked/${plan.id}/chunks/1`, bad, true);
	assert(r.status === 400 && /checksum/.test(r.json.message), `flipped bit refused (${r.status})`);
	r = await call('PUT', `/chunked/${plan.id}/chunks/2`, chunk(buf, 2, size).subarray(0, 1000), true);
	assert(r.status === 400 && /cut short/.test(r.json.message), `truncated chunk refused (${r.status})`);
	r = await call('PUT', `/chunked/${plan.id}/chunks/0`, Buffer.concat([chunk(buf, 0, size), Buffer.from('x')]), true);
	assert(r.status === 400, 'oversized chunk refused');
	const status = (await call('GET', `/chunked/${plan.id}`)).json;
	assert(JSON.stringify(status.missing) === '[0,1,2]', 'refused chunks still missing');
	r = await call('POST', `/chunked/${plan.id}/finish`);
	assert(r.status === 409, 'cannot finish with missing chunks');
	for (const i of [0, 1, 2]) assert((await call('PUT', `/chunked/${plan.id}/chunks/${i}`, chunk(buf, i, size), true)).status === 200, 'resend ok');
	const fin = (await call('POST', `/chunked/${plan.id}/finish`)).json;
	const dl = Buffer.from(await (await fetch(`${BASE}/api/nodes/${fin.node_id}/content`, { headers: { cookie } })).arrayBuffer());
	assert(sha(dl) === sha(buf), 'file intact after damaged attempts');
	ok('flipped bit, truncated and oversized chunks refused; file intact after resending');
}

// 3. Resume: half the chunks, then "the tab closed"; the same file planned again continues.
{
	const buf = randomBytes(10 * MIB);
	const size = MIB;
	const hashes = await planFor(buf, size);
	const body = { parent_id: folder.id, filename: 'resume.bin', size: buf.length, chunk_size: size, chunk_hashes: hashes };
	const first = (await call('POST', '/chunked', body)).json;
	for (const i of [0, 2, 4, 6, 8]) await call('PUT', `/chunked/${first.id}/chunks/${i}`, chunk(buf, i, size), true);
	const second = (await call('POST', '/chunked', body)).json;
	assert(second.id === first.id && second.resumed, 'same upload continued');
	assert(JSON.stringify(second.missing) === '[1,3,5,7,9]', `only the rest is missing (${second.missing})`);
	// A duplicate send of an already-received chunk is harmless.
	assert((await call('PUT', `/chunked/${first.id}/chunks/0`, chunk(buf, 0, size), true)).status === 200, 'duplicate chunk ok');
	for (const i of second.missing) await call('PUT', `/chunked/${first.id}/chunks/${i}`, chunk(buf, i, size), true);
	const fin = (await call('POST', `/chunked/${first.id}/finish`)).json;
	const dl = Buffer.from(await (await fetch(`${BASE}/api/nodes/${fin.node_id}/content`, { headers: { cookie } })).arrayBuffer());
	assert(sha(dl) === sha(buf), 'resumed file intact');
	ok('interrupted upload resumes: only missing chunks re-sent, result identical');

	// The same file planned again after finishing starts a new upload (a new copy).
	const third = (await call('POST', '/chunked', body)).json;
	assert(third.id !== first.id && !third.resumed, 'finished uploads are not resumed');
	await call('DELETE', `/chunked/${third.id}`);
	ok('a finished upload is not reused; cancel removes a started one');
}

// 4. Empty file.
{
	const plan = (await call('POST', '/chunked', { parent_id: folder.id, filename: 'empty.txt', size: 0, chunk_size: MIB, chunk_hashes: [] })).json;
	assert(plan.missing.length === 0, 'empty file needs no chunks');
	const fin = await call('POST', `/chunked/${plan.id}/finish`);
	const node = (await call('GET', `/nodes/${fin.json.node_id}`)).json.node;
	assert(node.size === 0 && node.name === 'empty.txt', 'empty file created');
	ok('empty file uploads');
}

// 5. Plan validation.
{
	const base = { parent_id: folder.id, filename: 'x.bin', size: 5 * MIB, chunk_size: MIB };
	const bad = [
		[{ ...base, chunk_hashes: ['a'.repeat(64)] }, 'wrong number of hashes'],
		[{ ...base, chunk_size: 3000, chunk_hashes: [] }, 'odd chunk size'],
		[{ ...base, chunk_hashes: Array(5).fill('Z'.repeat(64)) }, 'non-hex hash'],
		[{ ...base, filename: '../evil', chunk_hashes: Array(5).fill('a'.repeat(64)) }, 'path in file name'],
		[{ ...base, size: -1, chunk_hashes: [] }, 'negative size']
	];
	for (const [body, what] of bad) {
		const r = await call('POST', '/chunked', body);
		assert(r.status === 400, `${what} refused (${r.status})`);
	}
	const plan = (await call('POST', '/chunked', { ...base, chunk_hashes: Array(5).fill('a'.repeat(64)) })).json;
	const r = await call('PUT', `/chunked/${plan.id}/chunks/5`, Buffer.alloc(10), true);
	assert(r.status === 400, 'chunk index out of range refused');
	await call('DELETE', `/chunked/${plan.id}`);
	ok('bad plans and out-of-range chunks refused');
}

// 6. Another user cannot touch my upload; quota is enforced.
{
	const buf = randomBytes(2 * MIB);
	const hashes = await planFor(buf, MIB);
	const plan = (await call('POST', '/chunked', { parent_id: folder.id, filename: 'mine.bin', size: buf.length, chunk_size: MIB, chunk_hashes: hashes })).json;
	const username = `chunk${Date.now() % 100000}`;
	const created = await call('POST', '/admin/users', { username, display_name: 'Chunk Tester', password: 'chunkpass1' });
	assert(created.status === 200, 'member created');
	const other = await login(username, 'chunkpass1');
	const call2 = client(other.cookie);
	assert((await call2('PUT', `/chunked/${plan.id}/chunks/0`, chunk(buf, 0, MIB), true)).status === 404, 'other user cannot send chunks');
	assert((await call2('POST', '/chunked', { parent_id: folder.id, filename: 'x', size: 1, chunk_size: MIB, chunk_hashes: ['a'.repeat(64)] })).status === 404, 'other user cannot upload into my folder');
	await call('PATCH', `/admin/users/${created.json.id}`, { quota_bytes: MIB });
	const big = await call2('POST', '/chunked', { parent_id: other.me.home_id, filename: 'too-big.bin', size: 2 * MIB, chunk_size: MIB, chunk_hashes: hashes });
	assert(big.status === 413, `quota enforced (${big.status})`);
	await call('DELETE', `/admin/users/${created.json.id}`);
	await call('DELETE', `/chunked/${plan.id}`);
	ok('uploads are private to their owner; storage limit enforced');
}

await call('DELETE', `/nodes/${folder.id}`);
await call('DELETE', `/trash/${folder.id}`);
console.log(`All ${passed} chunked upload checks passed.`);
