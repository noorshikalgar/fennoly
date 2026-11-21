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
	allow_download: boolean;
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
	allow_download: boolean;
	shared_by: string;
	expires_at: string | null;
}

export interface PublicFolder {
	node: PublicNode;
	crumbs: { id: string; name: string }[];
	children: PublicNode[];
}
