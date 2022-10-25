import CreateDatabase from "./CreateDatabase";
import OpenDatabase from "./OpenDatabase";
import ViewDatabase from "./ViewDatabase";
import EntryEditor from "./EntryEditor";
import Settings from "./Settings";
import { atom, useRecoilState } from "recoil";
import { Entry } from "./state";
import * as ffi from "./ffi";

export enum AppStateVariant {
	Init,
	CreateDatabase,
	OpenDatabase,
	ViewDatabase,
	EditEntry,
	Settings,
}

export type AppState =
	| { variant: AppStateVariant.Init }
	| { variant: AppStateVariant.CreateDatabase }
	| { variant: AppStateVariant.OpenDatabase }
	| { variant: AppStateVariant.ViewDatabase }
	| { variant: AppStateVariant.EditEntry; entry: Entry | null }
	| { variant: AppStateVariant.Settings };

function app_state_to_component(state: AppState) {
	switch (state.variant) {
		case AppStateVariant.Init:
			return null;
		case AppStateVariant.CreateDatabase:
			return <CreateDatabase />;
		case AppStateVariant.OpenDatabase:
			return <OpenDatabase />;
		case AppStateVariant.ViewDatabase:
			return <ViewDatabase />;
		case AppStateVariant.EditEntry:
			return <EntryEditor entry={state.entry} />;
		case AppStateVariant.Settings:
			return <Settings />;
	}
}

export const appState = atom<AppState>({
	key: "appState",
	default: { variant: AppStateVariant.Init },
});

function App() {
	const [app, setAppState] = useRecoilState(appState);

	if (app.variant === AppStateVariant.Init) {
		void ffi.databaseExists().then((exists) => {
			setAppState(exists ? { variant: AppStateVariant.OpenDatabase } : { variant: AppStateVariant.CreateDatabase });
		});
	}

	return <div className="app">{app_state_to_component(app)}</div>;
}

export default App;
