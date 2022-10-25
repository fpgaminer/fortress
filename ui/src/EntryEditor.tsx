import { useEffect, useRef, useState } from "react";
import "./EntryEditor.css";
import { useRecoilValue, useSetRecoilState } from "recoil";
import { appState, AppStateVariant } from "./App";
import { Icon } from "@iconify/react";
import chevronLeft24Filled from "@iconify/icons-fluent/chevron-left-24-filled";
import dismiss24Filled from "@iconify/icons-fluent/dismiss-24-filled";
import save24Filled from "@iconify/icons-fluent/save-24-filled";
import eye24Filled from "@iconify/icons-fluent/eye-24-filled";
import eyeOff24Filled from "@iconify/icons-fluent/eye-off-24-filled";
import { DirectoryItemVariant } from "./DirectoryItem";
import { databaseState, Entry, refreshDatabase, ROOT_DIR_ID, selectedDirectoryState } from "./state";
import * as ffi from "./ffi";

function EntryEditor({ entry }: { entry: Entry | null }) {
	const [title, setTitle] = useState(entry?.state.title ?? "");
	const [username, setUsername] = useState(entry?.state.username ?? "");
	const [password, setPassword] = useState(entry?.state.password ?? null);
	const [showPassword, setShowPassword] = useState(false);
	const [url, setURL] = useState(entry?.state.url ?? "");
	const [notes, setNotes] = useState(entry?.state.notes ?? "");
	const setAppState = useSetRecoilState(appState);
	const selectedDirectory = useRecoilValue(selectedDirectoryState);
	const setDatabase = useSetRecoilState(databaseState);
	const [menu, setMenu] = useState({ x: 0, y: 0, open: false });
	const generateBtnRef = useRef<HTMLButtonElement>(null);

	function onBackClicked() {
		setAppState({ variant: AppStateVariant.ViewDatabase });
	}

	async function onDiscardClicked() {
		// At least on macOS confirm was returning a Promise<boolean> instead of a boolean, so we use Promise.resolve to handle both cases.
		const result = Promise.resolve(confirm("Are you sure you want to discard your changes?") as unknown);
		if (await result) {
			setAppState({ variant: AppStateVariant.ViewDatabase });
		}
	}

	async function onSaveClicked() {
		if (password === null) {
			return;
		}

		const parentId = selectedDirectory === DirectoryItemVariant.All ? ROOT_DIR_ID : selectedDirectory;

		try {
			await ffi.editEntry(
				entry?.id ?? null,
				{
					title: title,
					username: username,
					password: password,
					url: url,
					notes: notes,
				},
				parentId
			);
		} catch (e) {
			// TODO: This is a fatal error.  We should use a different dialog that allows the user to try and save again, or quit the application.
			await ffi.showErrorDialog(ffi.getErrorMessage(e));
		}

		setAppState({ variant: AppStateVariant.ViewDatabase });
		await refreshDatabase(setDatabase);
	}

	function onGenerateClicked(event: React.MouseEvent) {
		event.preventDefault();
		event.stopPropagation();

		// Place the menu at the bottom of the button.
		// x is relative to the right side of the window
		const rect = generateBtnRef.current?.getBoundingClientRect();
		if (rect) {
			setMenu({
				x: window.innerWidth - rect.right,
				y: rect.bottom,
				open: true,
			});
		}
	}

	function onCloseGenerateContextMenu() {
		setMenu({ x: 0, y: 0, open: false });
	}

	function onGenerate(pass: string) {
		setPassword(pass);
		onCloseGenerateContextMenu();
	}

	// Automatically generate a password for new entries
	if (password === null) {
		void defaultGeneratePassword(setPassword);
	}

	const modified =
		title != (entry?.state.title ?? "") ||
		username != (entry?.state.username ?? "") ||
		password != (entry?.state.password ?? "") ||
		url != (entry?.state.url ?? "") ||
		notes != (entry?.state.notes ?? "");

	return (
		<div className="entry-editor container">
			<div className="entry-editor-header">
				{modified ? null : (
					<button type="button" title="Go Back" onClick={onBackClicked}>
						<Icon icon={chevronLeft24Filled} className="icon" width="24" />
					</button>
				)}
				{modified ? (
					<button type="button" title="Discard Changes" onClick={onDiscardClicked}>
						<Icon icon={dismiss24Filled} className="icon" width="24" />
					</button>
				) : null}
				{modified ? (
					<button type="button" title="Save" onClick={onSaveClicked}>
						<Icon icon={save24Filled} className="icon" width="24" />
					</button>
				) : null}
			</div>
			<div className="entry-editor-main">
				<div className="field">
					<div className="label">Title</div>
					<input type="text" id="title" value={title} onChange={(e) => setTitle(e.currentTarget.value)} />
				</div>
				<div className="field">
					<div className="label">Username</div>
					<input type="text" id="username" value={username} onChange={(e) => setUsername(e.currentTarget.value)} />
				</div>
				<div className="field">
					<div className="label">Password</div>
					<div className="password-input">
						<input
							type={showPassword ? "text" : "password"}
							id="password"
							value={password ?? ""}
							onChange={(e) => setPassword(e.currentTarget.value)}
						/>
						<button className="show-password" title="Show password" onClick={() => setShowPassword(!showPassword)}>
							<Icon icon={showPassword ? eyeOff24Filled : eye24Filled} className="icon" width="24" />
						</button>
						<button type="button" title="Generate" onClick={onGenerateClicked} ref={generateBtnRef}>
							Generate
						</button>
					</div>
				</div>
				<div className="field">
					<div className="label">URL</div>
					<input type="url" id="url" value={url} onChange={(e) => setURL(e.currentTarget.value)} />
				</div>
				<div className="field">
					<div className="label">Notes</div>
					<textarea value={notes} onChange={(e) => setNotes(e.currentTarget.value)} />
				</div>
			</div>
			<GenerateMenu state={menu} onClose={onCloseGenerateContextMenu} onGenerate={onGenerate} />
		</div>
	);
}

// TODO: Should this reflect the current state of the generate dialog? Or maybe some kind of setting? Or at least break this out to global constants.
async function defaultGeneratePassword(setPassword: (password: string) => void) {
	setPassword(await ffi.randomString(20, true, true, true, ""));
}

function GenerateMenu({
	state,
	onClose,
	onGenerate,
}: {
	state: { x: number; y: number; open: boolean };
	onClose: () => void;
	onGenerate: (password: string) => void;
}) {
	const menu = useRef<HTMLElement>(null);
	const [length, setLength] = useState(20);
	const [uppercase, setUppercase] = useState(true);
	const [lowercase, setLowercase] = useState(true);
	const [numbers, setNumbers] = useState(true);
	const [others, setOthers] = useState("");

	function onMouseDownOutside(event: MouseEvent) {
		if (menu.current !== null && event.target instanceof Element && !menu.current.contains(event.target)) {
			event.preventDefault();
			event.stopPropagation();
			onClose();
		}
	}

	async function onGenerateClicked() {
		onGenerate(await ffi.randomString(length, uppercase, lowercase, numbers, others));
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
		<nav role="menu" tabIndex={-1} className="generate-menu" style={{ right: state.x, top: state.y }} ref={menu}>
			<label htmlFor="length">Length</label>
			<input
				type="number"
				min="1"
				max="1000"
				id="length"
				value={length}
				onChange={(e) => setLength(+e.currentTarget.value)}
			/>
			<div>
				<input
					type="checkbox"
					id="uppercase"
					checked={uppercase}
					onChange={(e) => setUppercase(e.currentTarget.checked)}
				/>
				<label htmlFor="uppercase">Uppercase (A-Z)</label>
			</div>
			<div>
				<input
					type="checkbox"
					id="lowercase"
					checked={lowercase}
					onChange={(e) => setLowercase(e.currentTarget.checked)}
				/>
				<label htmlFor="lowercase">Lowercase (a-z)</label>
			</div>
			<div>
				<input type="checkbox" id="numbers" checked={numbers} onChange={(e) => setNumbers(e.currentTarget.checked)} />
				<label htmlFor="numbers">Numbers (0-9)</label>
			</div>
			<label htmlFor="others">Other characters</label>
			<input type="text" id="others" value={others} onChange={(e) => setOthers(e.currentTarget.value)} />
			<button type="button" onClick={onGenerateClicked}>
				Generate
			</button>
		</nav>
	);
}

export default EntryEditor;
