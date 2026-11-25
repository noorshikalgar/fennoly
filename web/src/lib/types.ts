export type Permission = 'owner' | 'editor' | 'viewer';
export type Locale = 'en' | 'hi' | 'mr';
export type ThemeChoice = 'system' | 'light' | 'dark' | 'saffron';

export interface Me {
	id: string;
	username: string;
	display_name: string;
	role: 'admin' | 'member';
	locale: Locale;
	theme: ThemeChoice;
	avatar_color: string;
	quota_bytes: number | null;
	used_bytes: number;
	home_id: string | null;
	family_id: string | null;
}

export interface FileNode {
	id: string;
	parent_id: string | null;
	name: string;
	kind: 'folder' | 'file';
	size: number;
	mime: string | null;
	created_at: string;
	modified_at: string;
	trashed_at: string | null;
	starred: boolean;
	permission: Permission | null;
	thumbnail: boolean;
	is_root: boolean;
	parent_name: string | null;
	space_kind: 'personal' | 'family';
	parent_is_root: boolean;
}

export interface Crumb {
	id: string;
	name: string;
	is_root: boolean;
	space: 'personal' | 'family' | 'shared';
}

export interface NodeResponse {
	node: FileNode;
	crumbs: Crumb[];
}

export interface UserSummary {
	id: string;
	username: string;
	display_name: string;
	avatar_color: string;
}

export interface Share {
	id: string;
	node_id: string;
	permission: 'viewer' | 'editor';
	created_at: string;
	user_id: string;
	username: string;
	display_name: string;
	avatar_color: string;
}

export interface SharedItem extends FileNode {
	share_id: string;
	shared_by: string;
	shared_by_color: string;
	shared_at: string;
}

export interface AdminUser {
	id: string;
	username: string;
	display_name: string;
	role: 'admin' | 'member';
	avatar_color: string;
	quota_bytes: number | null;
	used_bytes: number;
	disabled: boolean;
	created_at: string;
	last_seen_at: string | null;
}

export interface ToolPreview {
	original_size: number;
	result_size: number;
	token: string;
}

export interface ShareLink {
	id: string;
	node_id: string;
	token: string;
	has_password: boolean;
	expires_at: string | null;
	expired: boolean;
	view_count: number;
	download_count: number;
	last_accessed_at: string | null;
	created_at: string;
}

export interface SharedByMeItem {
	node: FileNode;
	shares: Share[];
	links: ShareLink[];
	last_shared_at: string;
}

export interface PublicNode {
	id: string;
	name: string;
	kind: 'folder' | 'file';
	size: number;
	mime: string | null;
	modified_at: string;
	thumbnail: boolean;
}

export interface PublicInfo {
	root: PublicNode;
	shared_by: string;
	expires_at: string | null;
}

export interface PublicFolder {
	node: PublicNode;
	crumbs: { id: string; name: string }[];
	children: PublicNode[];
}

export type BackupKind = 'local' | 'smb' | 'sftp' | 's3';

export interface BackupSettings {
	path?: string;
	host?: string;
	port?: number;
	share?: string;
	user?: string;
	domain?: string;
	endpoint?: string;
	bucket?: string;
	region?: string;
}

export interface BackupSecrets {
	password?: string;
	access_key?: string;
	secret_key?: string;
}

export interface BackupRun {
	id: string;
	target_id: string;
	kind: 'backup' | 'check' | 'restore';
	status: 'running' | 'success' | 'failed';
	trigger: 'manual' | 'schedule';
	started_at: string;
	finished_at: string | null;
	snapshot_id: string | null;
	files_new: number | null;
	files_changed: number | null;
	bytes_added: number | null;
	total_files: number | null;
	total_bytes: number | null;
	error: string | null;
}

export interface BackupTarget {
	id: string;
	name: string;
	kind: BackupKind;
	settings: BackupSettings;
	has_password: boolean;
	has_keys: boolean;
	encrypted: boolean;
	schedule: string;
	keep_daily: number;
	keep_weekly: number;
	keep_monthly: number;
	include_trash: boolean;
	enabled: boolean;
	initialized: boolean;
	next_run_at: string | null;
	location: string;
	/** Progress (0–1) of the run in progress, or null. */
	running: number | null;
	last_run: BackupRun | null;
	last_success_at: string | null;
}

export interface BackupOverview {
	engine: { restic: boolean; rclone: boolean };
	targets: BackupTarget[];
}

export interface BackupSnapshot {
	id: string;
	short_id: string;
	time: string;
	paths: string[];
}

export interface BackupEntry {
	name: string;
	label: string | null;
	rel: string;
	is_dir: boolean;
	size: number | null;
	mtime: string | null;
}

export interface BackupStatus {
	targets: number;
	enabled: number;
	last_success_at: string | null;
	last_failed: boolean;
	stale: boolean;
}

export interface BackupTargetInput {
	name: string;
	kind?: BackupKind;
	settings: BackupSettings;
	secrets: BackupSecrets;
	encrypted?: boolean;
	recovery_key?: string;
	schedule: string;
	keep_daily: number;
	keep_weekly: number;
	keep_monthly: number;
	include_trash: boolean;
	enabled?: boolean;
}
