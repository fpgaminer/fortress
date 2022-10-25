import { useState } from "react";
import { useRecoilState, useSetRecoilState } from "recoil";
import * as ffi from "./ffi";
import { Icon } from "@iconify/react";
import folder24Filled from "@iconify/icons-fluent/folder-24-filled";
import { databaseState, Directory, refreshDatabase, selectedDirectoryState } from "./state";

export enum DirectoryItemVariant {
	Directory,
	All,
	New,
}

type DirectoryItemProps =
	| { variant: DirectoryItemVariant.Directory; directory: Directory }
	| { variant: DirectoryItemVariant.All }
	| { variant: DirectoryItemVariant.New; onCancel: () => void };

function DirectoryItem(props: DirectoryItemProps) {
	const setDatabase = useSetRecoilState(databaseState);
	const [selectedDirectory, setSelectedDirectory] = useRecoilState(selectedDirectoryState);
	const [renaming, setRenaming] = useState(props.variant === DirectoryItemVariant.New);

	function onDrop(event: React.DragEvent) {
		event.preventDefault();

		const entry_id = event.dataTransfer.getData("application/x.fortress.entry");

		if (entry_id.length != 64) {
			return;
		}

		if (props.variant === DirectoryItemVariant.Directory) {
			void doMove(entry_id, props.directory.id);
		}
	}

	async function doMove(entry_id: string, new_parent_id: string) {
		if (props.variant !== DirectoryItemVariant.Directory) {
			return;
		}

		try {
			await ffi.moveObject(entry_id, new_parent_id);
		} catch (e) {
			await ffi.showErrorDialog(ffi.getErrorMessage(e));
		}

		await refreshDatabase(setDatabase);
	}

	function allowDrop(event: React.DragEvent) {
		event.stopPropagation();
		event.preventDefault();
		event.dataTransfer.dropEffect = "move";
	}

	function onDoubleClick(event: React.MouseEvent) {
		if (props.variant !== DirectoryItemVariant.Directory) {
			return;
		}

		event.preventDefault();

		setRenaming(true);
	}

	function onBlur() {
		setRenaming(false);

		if (props.variant === DirectoryItemVariant.New) {
			props.onCancel();
		}
	}

	function onKeyUp(event: React.KeyboardEvent<HTMLInputElement>) {
		if (event.key == "Enter") {
			event.preventDefault();
			event.stopPropagation();

			void doRename(event.currentTarget.value);

			setRenaming(false);

			if (props.variant === DirectoryItemVariant.New) {
				props.onCancel();
			}
		} else if (event.key == "Escape") {
			event.preventDefault();
			event.stopPropagation();
			setRenaming(false);

			if (props.variant === DirectoryItemVariant.New) {
				props.onCancel();
			}
		}
	}

	async function doRename(new_name: string) {
		try {
			if (props.variant === DirectoryItemVariant.Directory) {
				await ffi.renameDirectory(props.directory.id, new_name);
			} else if (props.variant === DirectoryItemVariant.New) {
				await ffi.newDirectory(new_name);
			} else {
				return;
			}
		} catch (err) {
			await ffi.showErrorDialog(ffi.getErrorMessage(err));
		}

		await refreshDatabase(setDatabase);
	}

	function onClick() {
		if (props.variant === DirectoryItemVariant.Directory) {
			setSelectedDirectory(props.directory.id);
		} else if (props.variant === DirectoryItemVariant.All) {
			setSelectedDirectory(DirectoryItemVariant.All);
		}
	}

	const className =
		"directory-item" +
		((props.variant === DirectoryItemVariant.All && selectedDirectory === DirectoryItemVariant.All) ||
		(props.variant === DirectoryItemVariant.Directory && selectedDirectory == props.directory.id)
			? " selected"
			: "");
	const name =
		props.variant === DirectoryItemVariant.Directory
			? props.directory.name ?? ""
			: props.variant === DirectoryItemVariant.All
			? "All"
			: "";

	return (
		<div className={className} onDrop={onDrop} onDragOver={allowDrop} onClick={onClick} onDoubleClick={onDoubleClick}>
			<Icon icon={folder24Filled} width="24" className="icon" />
			{renaming ? (
				<input type="text" defaultValue={name} onBlur={onBlur} onKeyUp={onKeyUp} autoFocus />
			) : (
				<div className="directory-item-name">{name}</div>
			)}
		</div>
	);
}

export default DirectoryItem;
