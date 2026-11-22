import type {
	AdminUser,
	BackupEntry,
	BackupOverview,
	BackupRun,
	BackupSnapshot,
	BackupStatus,
	BackupTarget,
	BackupTargetInput,
	FileNode,
	Me,
	NodeResponse,
	PublicFolder,
	PublicInfo,
	Share,
	SharedByMeItem,
	SharedItem,
	ShareLink,
	ToolPreview,
	UserSummary
} from './types';

export class ApiError extends Error {
	constructor(
		public status: number,
		public code: string,
		message: string
	) {
		super(message);
	}
}

async function request<T>(method: string, path: string, body?: unknown): Promise<T> {
	const res = await fetch(`/api${path}`, {
		method,
		headers: body !== undefined ? { 'content-type': 'application/json' } : undefined,
		body: body !== undefined ? JSON.stringify(body) : undefined,
		credentials: 'same-origin'
	});
	if (!res.ok) {
		let code = 'error';
		let message = res.statusText;
		try {
			const data = await res.json();
			code = data.error ?? code;
			message = data.message ?? message;
		} catch {
			/* not JSON */
		}
		throw new ApiError(res.status, code, message);
	}
	return res.status === 204 ? (undefined as T) : res.json();
}

const get = <T>(p: string) => request<T>('GET', p);
const post = <T>(p: string, b?: unknown) => request<T>('POST', p, b ?? {});
const patch = <T>(p: string, b: unknown) => request<T>('PATCH', p, b);
const put = <T>(p: string) => request<T>('PUT', p, {});
const del = <T>(p: string) => request<T>('DELETE', p);

export const api = {
	setupStatus: () => get<{ needs_setup: boolean; version: string }>('/setup'),
	setup: (b: { username: string; display_name: string; password: string; locale: string }) =>
		post<Me>('/setup', b),
	login: (username: string, password: string) => post<Me>('/auth/login', { username, password }),
	logout: () => post('/auth/logout'),
	me: () => get<Me>('/me'),
	updateMe: (b: Partial<Pick<Me, 'display_name' | 'locale' | 'theme'>>) => patch<Me>('/me', b),
	changePassword: (current: string, next: string) => post('/me/password', { current, new: next }),
	users: () => get<UserSummary[]>('/users'),

	node: (id: string) => get<NodeResponse>(`/nodes/${id}`),
	children: (id: string, sort = 'name', order = 'asc') =>
		get<FileNode[]>(`/nodes/${id}/children?sort=${sort}&order=${order}`),
	createFolder: (parent: string, name: string) => post<FileNode>(`/nodes/${parent}/folders`, { name }),
	rename: (id: string, name: string) => patch<FileNode>(`/nodes/${id}`, { name }),
	move: (id: string, parent_id: string) => patch<FileNode>(`/nodes/${id}`, { parent_id }),
	trash: (id: string) => del(`/nodes/${id}`),
	star: (id: string, on: boolean) => (on ? put(`/nodes/${id}/star`) : del(`/nodes/${id}/star`)),
	recent: () => get<FileNode[]>('/recent'),
	starred: () => get<FileNode[]>('/starred'),
	search: (q: string) => get<FileNode[]>(`/search?q=${encodeURIComponent(q)}`),

	shares: (id: string) => get<Share[]>(`/nodes/${id}/shares`),
	share: (id: string, user_id: string, permission: string) =>
		post<Share[]>(`/nodes/${id}/shares`, { user_id, permission }),
	updateShare: (shareId: string, permission: string) => patch<Share[]>(`/shares/${shareId}`, { permission }),
	removeShare: (shareId: string) => del(`/shares/${shareId}`),
	sharedWithMe: () => get<SharedItem[]>('/shared'),
	sharedByMe: () => get<SharedByMeItem[]>('/shared-by-me'),
	stopSharing: (id: string) => del(`/nodes/${id}/sharing`),

	links: (id: string) => get<ShareLink[]>(`/nodes/${id}/links`),
	createLink: (id: string, b: { allow_download: boolean; password?: string; expires_at?: string | null }) =>
		post<ShareLink>(`/nodes/${id}/links`, b),
	updateLink: (linkId: string, b: { allow_download?: boolean; password?: string; expires_at?: string | null }) =>
		patch<ShareLink>(`/links/${linkId}`, b),
	deleteLink: (linkId: string) => del(`/links/${linkId}`),

	publicInfo: (token: string) => get<PublicInfo>(`/public/${token}`),
	publicUnlock: (token: string, password: string) => post(`/public/${token}/unlock`, { password }),
	publicFolder: (token: string, id: string) => get<PublicFolder>(`/public/${token}/folder/${id}`),

	trashList: () => get<FileNode[]>('/trash'),
	restore: (id: string) => post<FileNode>(`/trash/${id}/restore`),
	purge: (id: string) => del(`/trash/${id}`),
	emptyTrash: () => del<{ deleted: number }>('/trash'),

	chunkedPlan: (b: { parent_id: string; filename: string; size: number; chunk_size: number; chunk_hashes: string[] }) =>
		post<{ id: string; chunk_size: number; chunk_count: number; missing: number[]; resumed: boolean }>('/chunked', b),
	chunkedStatus: (id: string) =>
		get<{ missing: number[]; chunk_count: number; done: boolean; node_id: string | null }>(`/chunked/${id}`),
	chunkedFinish: (id: string) => post<{ node_id: string }>(`/chunked/${id}/finish`),
	chunkedCancel: (id: string) => del(`/chunked/${id}`),

	tools: () => get<{ image: boolean; pdf: boolean }>('/tools'),
	toolPreview: (b: { node_id: string; tool: 'image' | 'pdf'; quality?: number; max_dimension?: number | null; strength?: number }) =>
		post<ToolPreview>('/tools/preview', b),
	toolApply: (node_id: string, token: string, mode: 'copy' | 'replace') =>
		post<FileNode>('/tools/apply', { node_id, token, mode }),

	adminUsers: () => get<AdminUser[]>('/admin/users'),
	adminCreate: (b: { username: string; display_name: string; password: string; role: string; locale?: string }) =>
		post<AdminUser>('/admin/users', b),
	adminUpdate: (id: string, b: Record<string, unknown>) => patch<AdminUser>(`/admin/users/${id}`, b),
	adminDelete: (id: string) => del(`/admin/users/${id}`),

	backups: () => get<BackupOverview>('/admin/backups'),
	backupStatus: () => get<BackupStatus>('/admin/backups/status'),
	backupCreate: (b: BackupTargetInput) =>
		post<{ target: BackupTarget; recovery_key: string | null }>('/admin/backups', b),
	backupUpdate: (id: string, b: Partial<BackupTargetInput>) => patch<BackupTarget>(`/admin/backups/${id}`, b),
	backupDelete: (id: string) => del(`/admin/backups/${id}`),
	backupTest: (id: string) => post<{ ok: boolean }>(`/admin/backups/${id}/test`),
	backupRun: (id: string) => post<{ run_id: string }>(`/admin/backups/${id}/run`),
	backupRuns: (id: string, limit = 30) => get<BackupRun[]>(`/admin/backups/${id}/runs?limit=${limit}`),
	backupKey: (id: string, password: string) =>
		post<{ recovery_key: string | null }>(`/admin/backups/${id}/recovery-key`, { password }),
	backupSnapshots: (id: string) => get<BackupSnapshot[]>(`/admin/backups/${id}/snapshots`),
	backupBrowse: (id: string, snap: string, path: string) =>
		get<BackupEntry[]>(`/admin/backups/${id}/snapshots/${snap}/ls?path=${encodeURIComponent(path)}`),
	backupRestore: (id: string, snap: string, paths: string[]) =>
		post<{ run_id: string }>(`/admin/backups/${id}/snapshots/${snap}/restore`, { paths })
};

export const contentUrl = (id: string, download = false) =>
	`/api/nodes/${id}/content${download ? '?download=1' : ''}`;
export const thumbUrl = (n: FileNode) => `/api/nodes/${n.id}/thumbnail?v=${Date.parse(n.modified_at)}`;
export const zipUrl = (ids: string[]) => `/api/download/zip?ids=${ids.join(',')}`;

export const linkUrl = (token: string) => `${location.origin}/s/${token}`;
export const publicContentUrl = (token: string, id: string, download = false) =>
	`/api/public/${token}/content/${id}${download ? '?download=1' : ''}`;
export const publicThumbUrl = (token: string, id: string) => `/api/public/${token}/thumbnail/${id}`;
export const publicZipUrl = (token: string, ids: string[]) => `/api/public/${token}/zip?ids=${ids.join(',')}`;
