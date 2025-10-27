import { showModal } from "./common"
import { formatLogMsg } from "./logmsg"
import { saveQuery } from "./queries"
import { Collapsible, VList } from "./ui"
import { Histogram, HistogramItem } from "./histogram"
import { getQueryParam, removeQueryParam, setQueryParam } from "./utility"
import { Navbar } from "./navbar"

export type LogLevel = "trace" | "debug" | "info" | "warn" | "error" | "fatal"

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

export type SegmentProgressEvent = {
	type: "segment"
	segmentId: number
	deviceId?: string | null
	firstTimestamp: string
	lastTimestamp: string
	logsCount: number
}

export type SearchProgressEvent = {
	type: "stats"
	processedLogs: number
	logsPerSecond: number
}

export type ProgressEvent = SegmentProgressEvent | SearchProgressEvent

const isFiniteNumber = (value: unknown): value is number =>
	typeof value === "number" && Number.isFinite(value)

export const isSegmentProgressEvent = (
	value: unknown,
): value is SegmentProgressEvent => {
	if (typeof value !== "object" || value === null) return false
	const event = value as Partial<SegmentProgressEvent>
	if (event.type !== "segment") return false
	if (!isFiniteNumber(event.segmentId)) return false
	if (typeof event.firstTimestamp !== "string") return false
	if (typeof event.lastTimestamp !== "string") return false
	if (
		"logsCount" in event &&
		event.logsCount !== undefined &&
		!isFiniteNumber(event.logsCount)
	)
		return false
	if (
		"deviceId" in event &&
		event.deviceId !== undefined &&
		event.deviceId !== null &&
		typeof event.deviceId !== "string"
	)
		return false
	return true
}

export const isSearchProgressEvent = (
	value: unknown,
): value is SearchProgressEvent => {
	if (typeof value !== "object" || value === null) return false
	const event = value as Partial<SearchProgressEvent>
	if (event.type !== "stats") return false
	if (!isFiniteNumber(event.processedLogs)) return false
	if ("logsPerSecond" in event && event.logsPerSecond !== undefined)
		return isFiniteNumber(event.logsPerSecond)
	return true
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
	streamLogs: (
		args: FetchMoreArgs,
		onNewLog: (log: LogEntry) => void,
		onProgress: (progress: ProgressEvent) => void,
		onEnd: () => void,
	) => () => void
}

const MAX_LOG_ENTRIES = 10_000
const MESSAGE_TRUNCATE_LENGTH = 700
const OBSERVER_THRESHOLD = 0.1

const LOG_COLORS = {
	trace: "#6B7280",
	debug: "#3B82F6",
	info: "#10B981",
	warn: "#F59E0B",
	error: "#EF4444",
	fatal: "#8B5CF6",
} as const

export const searchSvg = `<svg xmlns="http://www.w3.org/2000/svg"  viewBox="0 0 50 50" width="20px" height="20px"><path d="M 21 3 C 11.601563 3 4 10.601563 4 20 C 4 29.398438 11.601563 37 21 37 C 24.355469 37 27.460938 36.015625 30.09375 34.34375 L 42.375 46.625 L 46.625 42.375 L 34.5 30.28125 C 36.679688 27.421875 38 23.878906 38 20 C 38 10.601563 30.398438 3 21 3 Z M 21 7 C 28.199219 7 34 12.800781 34 20 C 34 27.199219 28.199219 33 21 33 C 13.800781 33 8 27.199219 8 20 C 8 12.800781 13.800781 7 21 7 Z"/></svg>`

const formatTimestamp = (ts: string): string => {
	const date = new Date(ts)
	if (Number.isNaN(date.getTime())) return "unknown time"
	return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}-${String(date.getDate()).padStart(2, "0")} ${String(date.getHours()).padStart(2, "0")}:${String(date.getMinutes()).padStart(2, "0")}:${String(date.getSeconds()).padStart(2, "0")}`
}

const describeSegmentProgress = (progress: SegmentProgressEvent): string => {
	const start = formatTimestamp(progress.firstTimestamp)
	const end = formatTimestamp(progress.lastTimestamp)
	const device = progress.deviceId ? ` · ${progress.deviceId}` : ""
	const logs = progress.logsCount ? ` · ${progress.logsCount} logs` : ""
	return `Scanning segment ${progress.segmentId}${device} (${start} – ${end}${logs})`
}

const describeSearchProgress = (progress: SearchProgressEvent): string => {
	const processed = progress.processedLogs.toLocaleString()
	const speed = Number.isFinite(progress.logsPerSecond)
		? progress.logsPerSecond
		: 0
	const speedText = speed > 0 ? ` · ${speed.toFixed(1)} logs/sec` : ""
	return `Processed ${processed} logs${speedText}`
}

const escapeHTML = (str: string): string => {
	const div = document.createElement("div")
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
	args.root.innerHTML = ``

	const navbar = new Navbar()
	args.root.appendChild(navbar.root)

	const header = document.createElement("div")
	header.className = "page-header logs-header"
	args.root.appendChild(header)

	const headerControls = document.createElement("div")
	headerControls.className = "logs-header-controls"
	header.appendChild(headerControls)

	const searchTextarea = document.createElement("textarea")
	searchTextarea.className = "logs-search-bar"
	searchTextarea.placeholder = "Search logs (ctrl+enter to search)"
	searchTextarea.value = getQueryParam("query") || ""
	headerControls.appendChild(searchTextarea)

	const rightPanel = document.createElement("div")
	rightPanel.className = "logs-options-right-panel"
	headerControls.appendChild(rightPanel)

	const searchButton = document.createElement("button")
	searchButton.innerHTML = searchSvg
	searchButton.setAttribute("aria-busy", "false")

	const stopButton = document.createElement("button")
	stopButton.textContent = "Stop"
	stopButton.disabled = true
	stopButton.style.display = "none"

	const searchControls = document.createElement("div")
	searchControls.className = "logs-search-controls"
	searchControls.append(searchButton, stopButton)
	rightPanel.append(searchControls)

	// Options dropdown (currently only histogram)
	const featuresList = new VList()
	const histogramToggle = document.createElement("label")
	histogramToggle.style.display = "flex"
	histogramToggle.style.alignItems = "center"
	const histogramCheckbox = document.createElement("input")
	histogramCheckbox.type = "checkbox"
	histogramToggle.appendChild(histogramCheckbox)
	histogramToggle.appendChild(document.createTextNode(" Show histogram"))
	featuresList.root.appendChild(histogramToggle)
	const featuresDropdown = new Collapsible({
		buttonText: "Options",
		content: featuresList,
	})
	// If you want to show the dropdown uncomment next line
	// rightPanel.appendChild(featuresDropdown.root)

	// Histogram container
	const histogramContainer = document.createElement("div")
	histogramContainer.style.display = "none"
	const histogram = new Histogram()
	histogramContainer.appendChild(histogram.root)
	args.root.appendChild(histogramContainer)

	let histStream: null | (() => void) = null
	const startHistogram = () => {
		histogramContainer.style.display = "block"
		histogram.clear()
		const params = new URLSearchParams()
		if (searchTextarea.value) params.set("query", searchTextarea.value)
		params.set("bucketSecs", "60")
		params.set("tzOffset", new Date().getTimezoneOffset().toString())
		const url = new URL("/api/v1/logs/histogram", window.location.origin)
		url.search = params.toString()
		const es = new EventSource(url)
		es.onmessage = (ev) => {
			const item = JSON.parse(ev.data) as HistogramItem
			histogram.add(item)
		}
		es.onerror = () => es.close()
		histStream = () => es.close()
	}
	const stopHistogram = () => {
		if (histStream) histStream()
		histStream = null
		histogramContainer.style.display = "none"
		histogram.clear()
	}
	histogramCheckbox.onchange = () => {
		if (histogramCheckbox.checked) startHistogram()
		else stopHistogram()
	}

	// Top loading/progress row
	const loadingIndicator = document.createElement("div")
	loadingIndicator.className = "logs-loading-indicator"
	loadingIndicator.style.display = "none" // hidden until needed
	loadingIndicator.style.alignItems = "center"
	loadingIndicator.style.gap = "8px"
	loadingIndicator.style.padding = "4px 16px"
	loadingIndicator.style.fontSize = "12px"
	loadingIndicator.style.color = "#6b7280"
	loadingIndicator.style.justifyContent = "flex-start"

	const loadingSpinner = document.createElement("span")
	loadingSpinner.className = "logs-search-spinner"
	loadingSpinner.style.display = "none"

	const loadingText = document.createElement("span")
	loadingText.textContent = ""

	loadingIndicator.append(loadingSpinner, loadingText)
	header.appendChild(loadingIndicator)

	const setLoadingIndicator = (
		text: string,
		spinning: boolean,
		color?: string,
	) => {
		loadingText.textContent = text
		loadingSpinner.style.display = spinning ? "inline-block" : "none"
		if (color) loadingText.style.color = color
		else loadingText.style.color = "#6b7280"
		// Only show row if we have content (text or spinner)
		if (!text && !spinning) {
			loadingIndicator.style.display = "none"
		} else {
			loadingIndicator.style.display = "flex"
		}
	}

	let segmentStatus = ""
	let statsStatus = ""
	const updateProgressIndicator = () => {
		if (!segmentStatus && !statsStatus) {
			setLoadingIndicator("Searching…", true)
			return
		}
		const parts: string[] = []
		if (segmentStatus) parts.push(segmentStatus)
		if (statsStatus) parts.push(statsStatus)
		setLoadingIndicator(parts.join(" · "), true)
	}

	const logsList = document.createElement("div")
	logsList.className = "logs-list"
	args.root.appendChild(logsList)

	// Separate sentinel for infinite scroll
	const scrollSentinel = document.createElement("div")
	scrollSentinel.style.height = "1px"
	scrollSentinel.style.width = "100%"
	scrollSentinel.style.marginTop = "4px"
	args.root.appendChild(scrollSentinel)

	let debounce: any
	let pendingLogs: LogEntry[] = []

	const renderLogs = () => {
		// Keep list rows only (loading indicator is outside)
		logsList.innerHTML = logEntries
			.map(
				(entry) => `
			<div class="list-row">
				<div>
					${formatTimestamp(entry.timestamp)}
					<span style="color: ${LOG_COLORS[entry.level]}">${entry.level}</span>
					${entry.props.map((p) => `${p.key}=${p.value}`).join(" ")}
				</div>
				<div class="logs-list-row-msg">
					<div class="msg-summary">${escapeHTML(truncateMessage(entry.msg))}</div>
				</div>
			</div>
		`,
			)
			.join("")
		document.querySelectorAll(".msg-summary").forEach((el, key) => {
			el.addEventListener("click", () => {
				const entry = logEntries[key]
				const isTruncated = entry.msg.length > MESSAGE_TRUNCATE_LENGTH
				if (!isTruncated) return
				showModal({
					title: "Log Message",
					content: formatLogMsg(entry.msg),
					footer: [],
				})
			})
		})
	}

	const addLogs = (log: LogEntry) => {
		pendingLogs.push(log)
		if (debounce) return
		debounce = setTimeout(() => {
			const newEntries = pendingLogs.filter(
				(entry) => !logIds.has(entry.id),
			)
			newEntries.forEach((entry) => {
				logIds.add(entry.id)
				logEntries.push(entry)
			})
			logEntries.sort((a, b) => b.timestamp.localeCompare(a.timestamp))
			if (
				logEntries.length > MAX_LOG_ENTRIES &&
				args.root.scrollTop === 0
			) {
				const removed = logEntries.splice(MAX_LOG_ENTRIES)
				removed.forEach((r) => logIds.delete(r.id))
			}
			renderLogs()
			pendingLogs = []
			debounce = null
		}, 100)
	}

	const clearLogs = () => {
		logEntries.length = 0
		logIds.clear()
		renderLogs()
	}

	let currentStream: null | (() => void) = null
	let searchToken = 0

	const beginSearch = () => {
		const token = ++searchToken
		searchButton.disabled = true
		searchButton.setAttribute("aria-busy", "true")
		searchButton.style.display = "none"
		stopButton.disabled = false
		stopButton.style.display = "inline-flex"
		segmentStatus = ""
		statsStatus = ""
		updateProgressIndicator()
		return token
	}

	const finishSearch = (token: number, force = false) => {
		if (!force && token !== searchToken) return
		searchButton.disabled = false
		searchButton.setAttribute("aria-busy", "false")
		searchButton.style.display = "inline-flex"
		stopButton.disabled = true
		stopButton.style.display = "none"
		// hide spinner but preserve any existing status text (e.g. No logs found)
		loadingSpinner.style.display = "none"
		segmentStatus = ""
		statsStatus = ""
	}

	const stopSearch = () => {
		if (!currentStream) return
		const token = searchToken
		searchToken++
		currentStream()
		currentStream = null
		finishSearch(token, true)
		setLoadingIndicator("Search stopped", false)
	}

	const sentinelVisible = () => {
		const rect = scrollSentinel.getBoundingClientRect()
		return rect.top < window.innerHeight && rect.bottom >= 0
	}

	let streamRowsCount = 0
	let lastQuery = ""
	let lastEndDate: string | null = null

	const queryLogs = async (clear?: boolean) => {
		const query = searchTextarea.value
		if (query !== lastQuery) {
			const error = await args.validateQuery(query)
			if (error) {
				removeQueryParam("query")
				clearLogs()
				setLoadingIndicator(error, false, "red")
				return
			}
		}
		lastQuery = query
		if (query) setQueryParam("query", query)
		else removeQueryParam("query")

		let endDate: string | undefined
		if (logEntries.length > 0)
			endDate = logEntries[logEntries.length - 1].timestamp
		if (lastEndDate !== null && endDate === lastEndDate && !clear) {
			return
		}
		lastEndDate = endDate || null

		if (clear) clearLogs()
		if (histogramCheckbox.checked) {
			stopHistogram()
			startHistogram()
		}
		if (currentStream) currentStream()

		const token = beginSearch()
		streamRowsCount = 0
		currentStream = args.streamLogs(
			{ query, count: 200, endDate },
			(log) => {
				if (token !== searchToken) return
				streamRowsCount++
				addLogs(log)
			},
			(progress) => {
				if (token !== searchToken) return
				if (progress.type === "segment")
					segmentStatus = describeSegmentProgress(progress)
				else statsStatus = describeSearchProgress(progress)
				updateProgressIndicator()
			},
			() => {
				if (token !== searchToken) return
				currentStream = null
				if (streamRowsCount === 0) {
					if (logEntries.length === 0)
						setLoadingIndicator("No logs found", false)
					else setLoadingIndicator("No more logs", false)
				} else {
					setLoadingIndicator("", false)
				}
				finishSearch(token)
				if (streamRowsCount > 0 && sentinelVisible()) {
					// Automatically fetch next page if sentinel still visible
					queryLogs()
				}
			},
		)
	}

	searchTextarea.addEventListener("keydown", (e: KeyboardEvent) => {
		if (e.key === "Enter" && e.ctrlKey) {
			e.preventDefault()
			queryLogs(true)
		}
	})
	searchButton.addEventListener("click", () => queryLogs(true))
	stopButton.addEventListener("click", stopSearch)

	const observer = new IntersectionObserver(
		(entries) => {
			if (!entries[0].isIntersecting) return
			queryLogs()
		},
		{ threshold: OBSERVER_THRESHOLD },
	)
	observer.observe(scrollSentinel)
}
