import { invoke } from "@tauri-apps/api/core";

export interface DirectoryHistory {
	time: number;
	action: { Rename: string } | { Add: string } | { Remove: string };
}

export interface Directory {
	id: string;
	history: DirectoryHistory[];
}

export interface EntryHistory {
	time: number;
	data: Record<string, string | null>;
}

export interface Entry {
	id: string;
	history: EntryHistory[];
	time_created: number;
}

export function getErrorMessage(error: unknown) {
	if (error instanceof Error) {
		return error.message;
	}

	return String(error);
}

export async function showErrorDialog(message: string) {
	await invoke("error_dialog", { message });
}

export async function listDirectories(): Promise<Directory[]> {
	return await invoke("list_directories");
}

export async function listEntries(): Promise<Entry[]> {
	return await invoke("list_entries");
}

export async function renameDirectory(directory_id: string, new_name: string): Promise<void> {
	await invoke("rename_directory", { directoryId: directory_id, newName: new_name });
}

export async function moveObject(id: string, new_parent: string): Promise<void> {
	await invoke("move_object", { objectId: id, newParentId: new_parent });
}

export async function newDirectory(name: string): Promise<void> {
	await invoke("new_directory", { name });
}

export async function randomString(
	length: number,
	uppercase: boolean,
	lowercase: boolean,
	numbers: boolean,
	others: string,
): Promise<string> {
	return await invoke("random_string", { length, uppercase, lowercase, numbers, others });
}

export async function editEntry(entryId: string | null, data: Record<string, string>, parentId: string): Promise<void> {
	await invoke("edit_entry", { entryId, data, parentId });
}

export async function unlockDatabase(password: string): Promise<void> {
	await invoke("unlock_database", { password });
}

export async function databaseExists(): Promise<boolean> {
	return await invoke("database_exists");
}

export async function createDatabase(username: string, password: string): Promise<void> {
	await invoke("create_database", { username, password });
}

export async function getUsername(): Promise<string> {
	return await invoke("get_username");
}

export async function getSyncKeys(): Promise<string> {
	return await invoke("get_sync_keys");
}

export async function getSyncUrl(): Promise<string> {
	return await invoke("get_sync_url");
}

export async function setSyncUrl(url: string): Promise<void> {
	// TODO: This conflates Invalid URL error and database saving errors
	await invoke("set_sync_url", { url });
}

export async function changePassword(username: string, password: string): Promise<void> {
	await invoke("change_password", { username, password });
}

export async function syncDatabase(): Promise<void> {
	await invoke("sync_database");
}
