import { useState } from "react";
import "./Settings.css";
import { useSetRecoilState } from "recoil";
import { appState, AppStateVariant } from "./App";
import { Icon } from "@iconify/react";
import chevronLeft24Filled from "@iconify/icons-fluent/chevron-left-24-filled";
import eyeOff24Filled from "@iconify/icons-fluent/eye-off-24-filled";
import eye24Filled from "@iconify/icons-fluent/eye-24-filled";
import arrowSyncCircle24Filled from "@iconify/icons-fluent/arrow-sync-circle-24-filled";
import arrowSyncCheckmark24Filled from "@iconify/icons-fluent/arrow-sync-checkmark-24-filled";
import * as ffi from "./ffi";
import { databaseState, refreshDatabase } from "./state";

function Settings() {
	const setAppState = useSetRecoilState(appState);
	const setDatabase = useSetRecoilState(databaseState);
	const [syncUrl, setSyncUrlState] = useState<string | null>(null);
	const [username, setUsername] = useState<string | null>(null);
	const [password, setPassword] = useState("");
	const [repeatPassword, setRepeatPassword] = useState("");
	const [syncKeys, setSyncKeys] = useState<string | null>(null);
	const [showPassword, setShowPassword] = useState(false);
	const [showRepeatPassword, setShowRepeatPassword] = useState(false);
	const [showSyncKeys, setShowSyncKeys] = useState(false);
	const [syncing, setSyncing] = useState(0);
	const [changingPassword, setChangingPassword] = useState(0);

	async function onBackClicked() {
		if (syncUrl !== null) {
			try {
				await ffi.setSyncUrl(syncUrl);
			} catch (e) {
				await ffi.showErrorDialog(ffi.getErrorMessage(e));
			}
		}

		setAppState({ variant: AppStateVariant.ViewDatabase });
	}

	async function onSyncClicked() {
		setSyncing(1);

		try {
			if (syncUrl !== null) {
				await ffi.setSyncUrl(syncUrl);
			}

			await ffi.syncDatabase();

			setSyncing(2);
			await refreshDatabase(setDatabase);
			await sleep(1000);
		} catch (e) {
			await ffi.showErrorDialog(ffi.getErrorMessage(e));
		} finally {
			setSyncing(0);
		}
	}

	async function onLoginChangeClicked() {
		if (password != repeatPassword) {
			alert("Passwords do not match");
			return;
		}

		if (username === null) {
			return;
		}

		const result = Promise.resolve(confirm("Are you sure you want to change your username/password?") as unknown);
		if (!(await result)) {
			return;
		}

		setChangingPassword(1);

		try {
			await ffi.changePassword(username, password);

			setPassword("");
			setRepeatPassword("");
			setChangingPassword(2);
			await sleep(1000);
		} catch (e) {
			await ffi.showErrorDialog(ffi.getErrorMessage(e));
		} finally {
			setChangingPassword(0);
		}
	}

	if (username === null) {
		void ffi.getUsername().then((x) => setUsername(x));
	}

	if (syncKeys === null) {
		void ffi.getSyncKeys().then((x) => setSyncKeys(x));
	}

	if (syncUrl === null) {
		void ffi.getSyncUrl().then((x) => setSyncUrlState(x));
	}

	return (
		<div className="settings container">
			<div className="settings-header">
				<div>
					<button type="button" title="Go Back" onClick={onBackClicked}>
						<Icon icon={chevronLeft24Filled} className="icon" width="24" />
					</button>
				</div>
				<div>
					<h2>Settings</h2>
				</div>
				<div></div>
			</div>
			<div className="settings-main">
				<div className="settings-section">
					<h2>Sync</h2>
					<label htmlFor="sync_url">Sync URL</label>
					<input
						type="text"
						id="sync_url"
						value={syncUrl ?? ""}
						onChange={(e) => setSyncUrlState(e.currentTarget.value)}
					/>
					<button type="button" onClick={onSyncClicked} className="settings-btn" disabled={syncing > 0}>
						{syncing == 1 ? (
							<Icon icon={arrowSyncCircle24Filled} className="icon spinner" width="18" />
						) : syncing == 2 ? (
							<Icon icon={arrowSyncCheckmark24Filled} className="icon" width="18" />
						) : (
							"Sync"
						)}
					</button>
				</div>
				<div className="settings-section">
					<h2>Username and Password</h2>
					<label htmlFor="username">Username</label>
					<input
						type="text"
						id="username"
						value={username ?? ""}
						onChange={(e) => setUsername(e.currentTarget.value)}
					/>
					<label htmlFor="password">Password</label>
					<div>
						<input
							type={showPassword ? "text" : "password"}
							id="password"
							value={password}
							onChange={(e) => setPassword(e.currentTarget.value)}
						/>
						<button className="show-password" title="Show password" onClick={() => setShowPassword(!showPassword)}>
							<Icon icon={showPassword ? eyeOff24Filled : eye24Filled} className="icon" width="20" />
						</button>
					</div>
					<label htmlFor="repeat_password">Password (again)</label>
					<div>
						<input
							type={showRepeatPassword ? "text" : "password"}
							id="repeat_password"
							value={repeatPassword}
							onChange={(e) => setRepeatPassword(e.currentTarget.value)}
						/>
						<button
							className="show-password"
							title="Show password"
							onClick={() => setShowRepeatPassword(!showRepeatPassword)}
						>
							<Icon icon={showRepeatPassword ? eyeOff24Filled : eye24Filled} className="icon" width="20" />
						</button>
					</div>
					<button type="button" onClick={onLoginChangeClicked} className="settings-btn" disabled={changingPassword > 0}>
						{changingPassword == 1 ? (
							<Icon icon={arrowSyncCircle24Filled} className="icon spinner" width="18" />
						) : changingPassword == 2 ? (
							<Icon icon={arrowSyncCheckmark24Filled} className="icon" width="18" />
						) : (
							"Change"
						)}
					</button>
				</div>
				<div className="settings-section">
					<h2>Sync Keys</h2>
					<div>
						<input type={showSyncKeys ? "text" : "password"} id="sync_keys" value={syncKeys ?? ""} readOnly />
						<button className="show-password" title="Show sync keys" onClick={() => setShowSyncKeys(!showSyncKeys)}>
							<Icon icon={showSyncKeys ? eyeOff24Filled : eye24Filled} className="icon" width="24" />
						</button>
					</div>
				</div>
			</div>
		</div>
	);
}

function sleep(ms: number): Promise<void> {
	return new Promise((resolve) => setTimeout(resolve, ms));
}

export default Settings;
