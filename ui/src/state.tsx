import { atom } from "recoil";
import { DirectoryItemVariant } from "./DirectoryItem";
import * as ffi from "./ffi";

export const ROOT_DIR_ID = "0000000000000000000000000000000000000000000000000000000000000000";

export interface DatabaseState {
	directories: Directory[];
	entries: Entry[];
}

export const databaseState = atom<DatabaseState>({
	key: "database",
	default: {
		directories: [],
		entries: [],
	},
});

export const selectedDirectoryState = atom<string | DirectoryItemVariant.All>({
	key: "selectedDirectory",
	default: ROOT_DIR_ID,
});

export interface Directory {
	id: string;
	name: string | null;
	history: ffi.DirectoryHistory[];
	children: string[];
}

export interface Entry {
	id: string;
	history: ffi.EntryHistory[];
	time_created: number;
	state: Record<string, string | null>;
}

export async function refreshDatabase(setDatabase: (state: DatabaseState) => void) {
	const directories = (await ffi.listDirectories()).map((directory) => {
		let name = null;
		let children: string[] = [];

		for (const history of directory.history) {
			if ("Rename" in history.action) {
				name = history.action.Rename;
			} else if ("Add" in history.action) {
				children.push(history.action.Add);
			} else if ("Remove" in history.action) {
				const id = history.action.Remove;
				children = children.filter((child: string) => child !== id);
			}
		}

		return {
			id: directory.id,
			name,
			history: directory.history,
			children,
		};
	});

	const entries = (await ffi.listEntries()).map((entry) => {
		let state = {};

		for (const history of entry.history) {
			state = { ...state, ...history.data };
		}

		return {
			id: entry.id,
			history: entry.history,
			time_created: entry.time_created,
			state,
		};
	});

	setDatabase({ directories, entries });
}

export function getRootDirectory(directories: Directory[]): Directory {
	// Root directory always exists
	// eslint-disable-next-line @typescript-eslint/no-non-null-assertion
	return directories.find((directory) => directory.id == ROOT_DIR_ID)!;
}
