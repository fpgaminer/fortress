import "./OpenDatabase.css";
import { useState } from "react";
import fortressLogo from "./assets/fortress.svg";
import { useSetRecoilState } from "recoil";
import { appState, AppStateVariant } from "./App";
import { databaseState, refreshDatabase } from "./state";
import * as ffi from "./ffi";

function OpenDatabase() {
	const [password, setPassword] = useState("");
	const setAppState = useSetRecoilState(appState);
	const setDatabase = useSetRecoilState(databaseState);

	async function unlockClicked() {
		try {
			await ffi.unlockDatabase(password);

			setAppState({ variant: AppStateVariant.ViewDatabase });

			await refreshDatabase(setDatabase);
		} catch (e) {
			await ffi.showErrorDialog(ffi.getErrorMessage(e));
		}
	}

	return (
		<div className="open-database container">
			<h1>Welcome to Fortress</h1>
			<img src={fortressLogo} className="logo" alt="Fortress logo" />
			<p>Enter your password to unlock the Fortress.</p>

			<div className="row">
				<form
					onSubmit={(e) => {
						e.preventDefault();
						void unlockClicked();
					}}
				>
					<input
						type="password"
						id="password"
						onChange={(e) => setPassword(e.currentTarget.value)}
						placeholder="Enter your password..."
						autoFocus
					/>
					<button type="submit">Unlock</button>
				</form>
			</div>
		</div>
	);
}

export default OpenDatabase;
