import { navigate } from "./router";

export type LogLevel = 'trace' | 'debug' | 'info' | 'warn' | 'error' | 'fatal';

export type Prop = {
	key: string;
	value: string;
}

export type LogEntry = {
	id: string;
	timestamp: string;
	level: LogLevel;
	props: ReadonlyArray<Prop>;
	msg: string;
}

export type FetchMoreArgs = {
	offset?: number;
	count?: number;
	endDate?: string;
	query?: string;
}

interface LogsSearchPageArgs {
	root: HTMLElement
	isStreaming: boolean
	query?: string
	fetchMore: (args: FetchMoreArgs) => void
	toggleIsStreaming: () => boolean
}

const MAX_LOG_ENTRIES = 10_000;
const MESSAGE_TRUNCATE_LENGTH = 700;
const FETCH_DEBOUNCE_MS = 500;
const OBSERVER_THRESHOLD = 0.1;

const LOG_COLORS = {
	trace: "#6B7280",  // gray
	debug: "#3B82F6",  // blue
	info: "#10B981",   // green
	warn: "#F59E0B",   // orange
	error: "#EF4444",  // red
	fatal: "#8B5CF6"   // purple
} as const;

export const settingsSvg = `<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="lucide lucide-settings w-5 h-5"><path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z"></path><circle cx="12" cy="12" r="3"></circle></svg>`
export const searchSvg = `<svg xmlns="http://www.w3.org/2000/svg"  viewBox="0 0 50 50" width="20px" height="20px"><path d="M 21 3 C 11.601563 3 4 10.601563 4 20 C 4 29.398438 11.601563 37 21 37 C 24.355469 37 27.460938 36.015625 30.09375 34.34375 L 42.375 46.625 L 46.625 42.375 L 34.5 30.28125 C 36.679688 27.421875 38 23.878906 38 20 C 38 10.601563 30.398438 3 21 3 Z M 21 7 C 28.199219 7 34 12.800781 34 20 C 34 27.199219 28.199219 33 21 33 C 13.800781 33 8 27.199219 8 20 C 8 12.800781 13.800781 7 21 7 Z"/></svg>`

const formatTimestamp = (ts: string): string => {
	const date = new Date(ts);
	return date.toLocaleString();
};

const escapeHTML = (str: string): string => {
	const div = document.createElement('div');
	div.textContent = str;
	return div.innerHTML;
};

const truncateMessage = (msg: string): string =>
	msg.length > MESSAGE_TRUNCATE_LENGTH
		? `${msg.slice(0, MESSAGE_TRUNCATE_LENGTH)}...`
		: msg;

export const logsSearchPage = (args: LogsSearchPageArgs) => {
	const logIds = new Set<string>();
	const logEntries: LogEntry[] = [];
	let moreRows = true;

	args.root.innerHTML = ``

	const logsOptions = document.createElement("div")
	logsOptions.className = "logs-options"
	args.root.appendChild(logsOptions)
	const searchTextarea = document.createElement("textarea")
	searchTextarea.className = "logs-search-bar"
	searchTextarea.placeholder = "Search logs (ctrl+enter to search)"
	searchTextarea.value = args.query || ""
	logsOptions.appendChild(searchTextarea)
	const optionsRightPanel = document.createElement("div")
	optionsRightPanel.className = "logs-options-right-panel"
	logsOptions.appendChild(optionsRightPanel)
	const settingsButton = document.createElement("button")
	settingsButton.innerHTML = settingsSvg
	settingsButton.onclick = () => navigate("/settings")
	const searchButton = document.createElement("button")
	searchButton.innerHTML = searchSvg
	const streamButton = document.createElement("button")
	streamButton.innerHTML = "stream"
	optionsRightPanel.append(settingsButton, searchButton, streamButton)

	const logsList = document.createElement("div")
	logsList.className = "logs-list"
	args.root.appendChild(logsList)
	const loadingIndicator = document.createElement("div")
	args.root.appendChild(loadingIndicator)

	const queryLogs = (clear?: boolean): void => {
		loadingIndicator.textContent = "Loading..."
		let endDate
		if (logEntries.length > 0) endDate = logEntries[logEntries.length - 1].timestamp
		if (clear) {
			console.log("clearing logs")
			logEntries.length = 0
			logIds.clear()
			logsList.innerHTML = ""
		}
		args.fetchMore({ 
			count: 100, 
			query: searchTextarea.value,
			endDate: endDate
		})
	};

	searchTextarea.addEventListener("keydown", (e: KeyboardEvent) => {
		if (e.key === "Enter" && e.ctrlKey) {
			e.preventDefault()
			queryLogs(true)
		}
	})
	searchButton.addEventListener("click", () => queryLogs(true))
	const updateStreamButtonText = (isStreaming: boolean) => {
		streamButton.innerHTML = isStreaming ? "stop" : "stream"
	}
	streamButton.addEventListener("click", () => {
		const isStreaming = args.toggleIsStreaming();
		updateStreamButtonText(isStreaming);
	});
	const observer = new IntersectionObserver(
		(entries) => {
			if (!moreRows || !entries[0].isIntersecting) return;
			moreRows = false;
			queryLogs()
		},
		{
			threshold: OBSERVER_THRESHOLD
		}
	);
	observer.observe(loadingIndicator);

	// Return public interface
	return {
		setIsStreaming: updateStreamButtonText,
		onError(err: string): void {
			loadingIndicator.textContent = err;
		},
		addLogEntries(entries: ReadonlyArray<LogEntry>): void {
			if (entries.length === 0) {
				loadingIndicator.textContent = 'No more rows';
				return;
			}
			setTimeout(() => { moreRows = true; }, FETCH_DEBOUNCE_MS);
			const newEntries = entries.filter(entry => !logIds.has(entry.id));
			newEntries.forEach(entry => {
				logIds.add(entry.id);
				logEntries.push(entry);
			});
			logEntries.sort((a, b) => b.timestamp.localeCompare(a.timestamp));
			if (logEntries.length > MAX_LOG_ENTRIES && args.root.scrollTop === 0) {
				const removed = logEntries.splice(MAX_LOG_ENTRIES);
				removed.forEach(r => logIds.delete(r.id));
			}

			logsList.innerHTML = logEntries.map(entry => `
				<div class="logs-list-row">
					<div>
						${formatTimestamp(entry.timestamp)} 
						<span style="color: ${LOG_COLORS[entry.level]}">${entry.level}</span>
						${entry.props.map(p => `${p.key}=${p.value}`).join(" ")}
					</div>
					<div class="logs-list-row-msg" title="${entry.msg}">
						${escapeHTML(truncateMessage(entry.msg))}
					</div>
				</div>
			`).join('');
		}
	};
};