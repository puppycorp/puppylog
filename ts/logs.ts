import { showModal } from "./common"
import { formatLogMsg } from "./logmsg"
import { navigate } from "./router"
import { Button } from "./ui"
import { saveQuery } from "./queries"
import { getQueryParam, removeQueryParam, setQueryParam } from "./utility"

export type LogLevel = 'trace' | 'debug' | 'info' | 'warn' | 'error' | 'fatal'

export type Prop = {
	key: string
	value: string
}

export type LogEntry = {
	id: string
	timestamp: string
	level: LogLevel
	props: ReadonlyArray<Prop>
	msg: string
}

export type FetchMoreArgs = {
	offset?: number
	count?: number
	endDate?: string
	query?: string
}

interface LogsSearchPageArgs {
	root: HTMLElement
	validateQuery: (query: string) => Promise<string | null>
	streamLogs: (args: FetchMoreArgs, onNewLog: (log: LogEntry) => void, onEnd: () => void) => () => void
}

const MAX_LOG_ENTRIES = 10_000
const MESSAGE_TRUNCATE_LENGTH = 700
const FETCH_DEBOUNCE_MS = 500
const OBSERVER_THRESHOLD = 0.1

const LOG_COLORS = {
	trace: "#6B7280",  // gray
	debug: "#3B82F6",  // blue
	info: "#10B981",   // green
	warn: "#F59E0B",   // orange
	error: "#EF4444",  // red
	fatal: "#8B5CF6"   // purple
} as const

export const settingsSvg = `<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="lucide lucide-settings w-5 h-5"><path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z"></path><circle cx="12" cy="12" r="3"></circle></svg>`
export const searchSvg = `<svg xmlns="http://www.w3.org/2000/svg"  viewBox="0 0 50 50" width="20px" height="20px"><path d="M 21 3 C 11.601563 3 4 10.601563 4 20 C 4 29.398438 11.601563 37 21 37 C 24.355469 37 27.460938 36.015625 30.09375 34.34375 L 42.375 46.625 L 46.625 42.375 L 34.5 30.28125 C 36.679688 27.421875 38 23.878906 38 20 C 38 10.601563 30.398438 3 21 3 Z M 21 7 C 28.199219 7 34 12.800781 34 20 C 34 27.199219 28.199219 33 21 33 C 13.800781 33 8 27.199219 8 20 C 8 12.800781 13.800781 7 21 7 Z"/></svg>`

const formatTimestamp = (ts: string): string => {
	const date = new Date(ts)
	return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, '0')}-${String(date.getDate()).padStart(2, '0')} ${String(date.getHours()).padStart(2, '0')}:${String(date.getMinutes()).padStart(2, '0')}:${String(date.getSeconds()).padStart(2, '0')}`
}

const escapeHTML = (str: string): string => {
	const div = document.createElement('div')
	div.textContent = str
	return div.innerHTML
}

const truncateMessage = (msg: string): string =>
	msg.length > MESSAGE_TRUNCATE_LENGTH
		? `${msg.slice(0, MESSAGE_TRUNCATE_LENGTH)}...`
		: msg

export const logsSearchPage = (args: LogsSearchPageArgs) => {
	const logIds = new Set<string>()
	const logEntries: LogEntry[] = []
	let moreRows = true
	args.root.innerHTML = ``

	const logsOptions = document.createElement("div")
	logsOptions.className = "page-header"
	args.root.appendChild(logsOptions)

	const searchTextarea = document.createElement("textarea")
	searchTextarea.className = "logs-search-bar"
	searchTextarea.placeholder = "Search logs (ctrl+enter to search)"
	searchTextarea.value = getQueryParam("query") || ""
	logsOptions.appendChild(searchTextarea)

	const optionsRightPanel = document.createElement("div")
	optionsRightPanel.className = "logs-options-right-panel"
	logsOptions.appendChild(optionsRightPanel)

    const settingsButton = document.createElement("button")
    settingsButton.innerHTML = settingsSvg
    settingsButton.onclick = () => navigate("/settings")

    const saveButton = document.createElement("button")
    saveButton.textContent = "Save"
    saveButton.onclick = () => {
            const query = searchTextarea.value.trim()
            if (!query) return
            const name = prompt("Query name", query)
            if (name) saveQuery({ name, query })
    }

    const searchButton = document.createElement("button")
    searchButton.innerHTML = searchSvg

    optionsRightPanel.append(settingsButton, saveButton, searchButton)
	const logsList = document.createElement("div")
	logsList.className = "logs-list"
	args.root.appendChild(logsList)
	const loadingIndicator = document.createElement("div")
	loadingIndicator.style.height = "50px"
	args.root.appendChild(loadingIndicator)
	let debounce: any
	let pendingLogs: LogEntry[] = []
	const addLogs = (log: LogEntry) => {
		pendingLogs.push(log)
		if (debounce) return
		debounce = setTimeout(() => {
			const newEntries = pendingLogs.filter(entry => !logIds.has(entry.id))
			newEntries.forEach(entry => {
				logIds.add(entry.id)
				logEntries.push(entry)
			})
			logEntries.sort((a, b) => b.timestamp.localeCompare(a.timestamp))
			if (logEntries.length > MAX_LOG_ENTRIES && args.root.scrollTop === 0) {
				const removed = logEntries.splice(MAX_LOG_ENTRIES)
				removed.forEach(r => logIds.delete(r.id))
			}
			logsList.innerHTML = logEntries.map(entry => `
				<div class="list-row">
					<div>
						${formatTimestamp(entry.timestamp)} 
						<span style="color: ${LOG_COLORS[entry.level]}">${entry.level}</span>
						${entry.props.map(p => `${p.key}=${p.value}`).join(" ")}
					</div>
					<div class="logs-list-row-msg">
						<div class="msg-summary">${escapeHTML(truncateMessage(entry.msg))}</div>
					</div>
				</div>
			`).join('')
			document.querySelectorAll(".list-row").forEach((el, key) => {
				el.addEventListener("click", () => {
					const entry = logEntries[key]
					const copyButton = new Button({ text: "Copy" })
					copyButton.onClick = () => navigator.clipboard.writeText(entry.msg)
					const closeButton = new Button({ text: "Close" })
					showModal({
						title: "Log Message",
						content: formatLogMsg(entry.msg),
						footer: [
							copyButton
						]
					})
				})
			})
			pendingLogs = []
			debounce = null
		}, 100)
	}
	const clearLogs = () => {
		logEntries.length = 0
		logIds.clear()
		logsList.innerHTML = ""
	}
	let currentStream: null | (() => void) = null
	const loadingIndicatorVisible = () => {
		const rect = loadingIndicator.getBoundingClientRect()
		return rect.top < window.innerHeight && rect.bottom >= 0
	}
	let streamRowsCount = 0
	let lastQuery = ""
	let lastEndDate: any = null
	const queryLogs = async (clear?: boolean) => {
		const query = searchTextarea.value
		if (query !== lastQuery) {
			const error = await args.validateQuery(query)
			if (error) {
				removeQueryParam("query")
				logsList.innerHTML = ""
				loadingIndicator.innerHTML = `<div style="color: red">${error}</div>`
				return
			}
		}
		lastQuery = query
		if (query) setQueryParam("query", query)
		else removeQueryParam("query")
		loadingIndicator.textContent = "Loading..."
		let endDate
		if (logEntries.length > 0) endDate = logEntries[logEntries.length - 1].timestamp
		if (lastEndDate !== null && endDate === lastEndDate) return
		lastEndDate = endDate
		if (clear) clearLogs()
		if (currentStream) currentStream()
		currentStream = args.streamLogs(
			{ query, count: 200, endDate },
			(log) => {
				streamRowsCount++
				addLogs(log)
			},
			() => {
				currentStream = null
				loadingIndicator.textContent = ""
				console.log("stream rows count", streamRowsCount)
				if (streamRowsCount === 0) {
					loadingIndicator.textContent = logEntries.length === 0 ? "No logs found" : "No more logs"
					return
				}
				streamRowsCount = 0
				if (loadingIndicatorVisible()) queryLogs()
			}
		)
	}

	searchTextarea.addEventListener("keydown", (e: KeyboardEvent) => {
		if (e.key === "Enter" && e.ctrlKey) {
			e.preventDefault()
			queryLogs(true)
		}
	})
	searchButton.addEventListener("click", () => queryLogs(true))
	const observer = new IntersectionObserver(
		(entries) => {
			if (!entries[0].isIntersecting) return
			queryLogs()
		},
		{ threshold: OBSERVER_THRESHOLD }
	)
	observer.observe(loadingIndicator)
}