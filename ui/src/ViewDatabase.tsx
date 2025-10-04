import React, { useEffect, useRef, useState } from "react";
import "./ViewDatabase.css";
import { useRecoilState, useRecoilValue, useSetRecoilState } from "recoil";
import { Icon } from "@iconify/react";
import settings24Filled from "@iconify/icons-fluent/settings-24-filled";
import add24Filled from "@iconify/icons-fluent/add-24-filled";
import folderAdd24Filled from "@iconify/icons-fluent/folder-add-24-filled";
import { appState, AppStateVariant } from "./App";
import DirectoryItem, { DirectoryItemVariant } from "./DirectoryItem";
import { databaseState, Directory, Entry, getRootDirectory, ROOT_DIR_ID, selectedDirectoryState } from "./state";

function sortDirectories(directories: Directory[]) {
	const directory_name = (directory: Directory) => directory.name ?? "Unnamed";

	if (directories.length === 0) {
		return [];
	}

	// root is added later
	const result = directories.filter((directory) => directory.id !== ROOT_DIR_ID);

	// Sort by name
	result.sort((a, b) => {
		return directory_name(a).localeCompare(directory_name(b));
	});

	// Root folder at the top
	result.unshift(getRootDirectory(directories));

	return result;
}

function filterEntriesByDirectory(
	entries: Entry[],
	directories: Directory[],
	selectedDirectory: string | DirectoryItemVariant.All,
) {
	if (selectedDirectory === DirectoryItemVariant.All) {
		return entries.slice();
	}

	const directory = directories.find((directory) => directory.id === selectedDirectory);

	if (directory === undefined) {
		return entries.slice();
	}

	return entries.filter((entry) => directory.children.includes(entry.id));
}

function filterEntriesBySearch(entries: Entry[], search: string) {
	if (search === "") {
		return entries.slice();
	}

	return entries.filter((entry) => {
		const searchLower = search.toLowerCase();

		return (entry.state.title ?? "").toLowerCase().includes(searchLower);
	});
}

function sortEntries(entries: Entry[]) {
	const result = entries.slice();

	// Sort by time created (and then by ID as a tie breaker)
	result.sort((a, b) => {
		if (a.time_created === b.time_created) {
			return b.id.localeCompare(a.id);
		} else {
			return b.time_created - a.time_created;
		}
	});

	return result;
}

function ViewDatabase() {
	const [search, setSearch] = useState("");
	const [contextMenuEntry, setContextMenuEntry] = useState("");
	const [menu, setMenu] = useState({ x: 0, y: 0, open: false });
	const database = useRecoilValue(databaseState);
	const [selectedDirectory, setSelectedDirectory] = useRecoilState(selectedDirectoryState);
	const setAppState = useSetRecoilState(appState);
	const [newDirectory, setNewDirectory] = useState(false);

	function handleContextMenu(event: React.MouseEvent, entry_id: string) {
		event.preventDefault();
		event.stopPropagation();

		const x = event.pageX;
		const y = event.pageY;

		setMenu({ x: x, y: y, open: true });
		setContextMenuEntry(entry_id);
	}

	function onCloseContextMenu() {
		setMenu({ x: 0, y: 0, open: false });
	}

	function onCopyUsername() {
		const entry = database.entries.find((entry) => entry.id === contextMenuEntry);

		if (entry) {
			void navigator.clipboard.writeText(entry.state.username ?? "");
		}

		onCloseContextMenu();
	}

	function onCopyPassword() {
		const entry = database.entries.find((entry) => entry.id === contextMenuEntry);

		if (entry) {
			void navigator.clipboard.writeText(entry.state.password ?? "");
		}

		onCloseContextMenu();
	}

	function onCopyUrl() {
		const entry = database.entries.find((entry) => entry.id === contextMenuEntry);

		if (entry) {
			void navigator.clipboard.writeText(entry.state.url ?? "");
		}

		onCloseContextMenu();
	}

	function onAddEntryClicked() {
		setAppState({ variant: AppStateVariant.EditEntry, entry: null });
	}

	function onAddDirectoryClicked() {
		setNewDirectory(true);
	}

	function onSettingsClicked() {
		setAppState({ variant: AppStateVariant.Settings });
	}

	function onCancelNewDirectory() {
		setNewDirectory(false);
	}

	function onSearchChange(event: React.ChangeEvent<HTMLInputElement>) {
		if (search == "" && event.target.value != "") {
			setSelectedDirectory(DirectoryItemVariant.All);
		}

		setSearch(event.currentTarget.value);
	}

	const dirs = sortDirectories(database.directories).map((dir) => (
		<DirectoryItem key={dir.id} variant={DirectoryItemVariant.Directory} directory={dir} />
	));

	// The "All" category
	dirs.unshift(<DirectoryItem key={""} variant={DirectoryItemVariant.All} />);

	// New directory
	if (newDirectory) {
		dirs.push(<DirectoryItem key={"new"} variant={DirectoryItemVariant.New} onCancel={onCancelNewDirectory} />);
	}

	const entries_filtered = sortEntries(
		filterEntriesBySearch(filterEntriesByDirectory(database.entries, database.directories, selectedDirectory), search),
	);

	const ents = entries_filtered.map((ent) => (
		<EntryItem key={ent.id} entry={ent} onContextMenu={(e: React.MouseEvent) => handleContextMenu(e, ent.id)} />
	));

	return (
		<div className="view-database container">
			<div className="view-database-header">
				<input
					type="search"
					placeholder="Search..."
					onChange={onSearchChange}
					autoFocus
					autoComplete="off"
					spellCheck="false"
					autoCorrect="off"
				/>
				<button type="button" title="Add Entry" onClick={onAddEntryClicked}>
					<Icon icon={add24Filled} className="icon" width="24" />
				</button>
				<button type="button" title="Add Directory" onClick={onAddDirectoryClicked}>
					<Icon icon={folderAdd24Filled} className="icon" width="24" />
				</button>
				<button type="button" title="Settings" onClick={onSettingsClicked}>
					<Icon icon={settings24Filled} className="icon" width="24" />
				</button>
			</div>
			<div className="view-database-main">
				<div className="directories">{dirs}</div>
				<div className="entries">
					<table>
						<thead>
							<tr>
								<th>
									<div>Title</div>
								</th>
								<th>
									<div>Username</div>
								</th>
								<th>
									<div>URL</div>
								</th>
							</tr>
						</thead>
						<tbody>{ents}</tbody>
					</table>
					<ContextMenu
						state={menu}
						onClose={() => onCloseContextMenu()}
						onCopyUsername={onCopyUsername}
						onCopyPassword={onCopyPassword}
						onCopyUrl={onCopyUrl}
					/>
				</div>
			</div>
		</div>
	);
}

function EntryItem({ entry, onContextMenu }: { entry: Entry; onContextMenu: (event: React.MouseEvent) => void }) {
	const setAppState = useSetRecoilState(appState);
	const title = entry.state.title || "-";
	const username = entry.state.username || "-";
	const url = entry.state.url || "-";

	function onDragStart(event: React.DragEvent) {
		event.dataTransfer.setData("application/x.fortress.entry", entry.id);
		event.dataTransfer.effectAllowed = "move";
	}

	function onDoubleClick(event: React.MouseEvent) {
		event.preventDefault();

		setAppState({ variant: AppStateVariant.EditEntry, entry: entry });
	}

	return (
		<tr
			className="entry-item"
			onContextMenu={onContextMenu}
			draggable={true}
			onDragStart={(e) => onDragStart(e)}
			onDoubleClick={onDoubleClick}
		>
			<td>
				<div className="entry-item-title">{title}</div>
			</td>
			<td>
				<div className="entry-item-username">{username}</div>
			</td>
			<td>
				<div className="entry-item-url">{url}</div>
			</td>
		</tr>
	);
}

function ContextMenu({
	state,
	onClose,
	onCopyUsername,
	onCopyPassword,
	onCopyUrl,
}: {
	state: { x: number; y: number; open: boolean };
	onClose: () => void;
	onCopyUsername: () => void;
	onCopyPassword: () => void;
	onCopyUrl: () => void;
}) {
	const menu = useRef<HTMLElement>(null);

	function onMouseDownOutside(event: MouseEvent) {
		if (menu.current !== null && event.target instanceof Element && !menu.current.contains(event.target)) {
			onClose();
		}
	}

	useEffect(() => {
		window.addEventListener("click", onMouseDownOutside);
		return () => {
			window.removeEventListener("click", onMouseDownOutside);
		};
	});

	if (!state.open) {
		return null;
	}

	return (
		<nav role="menu" tabIndex={-1} className="context-menu" style={{ left: state.x, top: state.y }} ref={menu}>
			<div className="context-menu-item" role="menuitem" tabIndex={-1} onClick={onCopyUsername}>
				Copy Username
			</div>
			<div className="context-menu-item" role="menuitem" tabIndex={-1} onClick={onCopyPassword}>
				Copy Password
			</div>
			<div className="context-menu-item" role="menuitem" tabIndex={-1} onClick={onCopyUrl}>
				Copy URL
			</div>
		</nav>
	);
}

export default ViewDatabase;
