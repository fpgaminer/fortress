import "./CreateDatabase.css";
import { useState } from "react";
import fortressLogo from "./assets/fortress.svg";
import { useSetRecoilState } from "recoil";
import { appState, AppStateVariant } from "./App";
import { databaseState, refreshDatabase } from "./state";
import * as ffi from "./ffi";
import arrowSyncCircle24Filled from "@iconify/icons-fluent/arrow-sync-circle-24-filled";
import { Icon } from "@iconify/react";

function CreateDatabase() {
	const [username, setUsername] = useState("");
	const [password, setPassword] = useState("");
	const [passwordRepeat, setPasswordRepeat] = useState("");
	const setAppState = useSetRecoilState(appState);
	const setDatabase = useSetRecoilState(databaseState);
	const [creating, setCreating] = useState(false);

	function createClicked(event: React.FormEvent) {
		event.preventDefault();

		if (password !== passwordRepeat) {
			void ffi.showErrorDialog("Passwords do not match");
			return;
		}

		void doCreate(username, password);
	}

	async function doCreate(username: string, password: string) {
		setCreating(true);

		try {
			await ffi.createDatabase(username, password);
			await refreshDatabase(setDatabase);

			setAppState({ variant: AppStateVariant.ViewDatabase });
		} catch (e) {
			await ffi.showErrorDialog(ffi.getErrorMessage(e));
		} finally {
			setCreating(false);
		}
	}

	return (
		<div className="create-database container">
			<h1>Welcome to Fortress</h1>
			<img src={fortressLogo} className="logo" alt="Fortress logo" />
			<p>Enter a username and password to create your Fortress.</p>

			<div className="row">
				<form onSubmit={createClicked}>
					<input type="text" onChange={(e) => setUsername(e.currentTarget.value)} placeholder="Username..." autoFocus />
					<input type="password" onChange={(e) => setPassword(e.currentTarget.value)} placeholder="Password..." />
					<input
						type="password"
						onChange={(e) => setPasswordRepeat(e.currentTarget.value)}
						placeholder="Password (repeat)..."
					/>
					<button type="submit" disabled={creating}>
						{creating ? <Icon icon={arrowSyncCircle24Filled} className="icon spinner" width="18" /> : "Create"}
					</button>
				</form>
			</div>
		</div>
	);
}

export default CreateDatabase;
