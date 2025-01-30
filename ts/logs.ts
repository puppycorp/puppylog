import { getQueryParam } from "./utility"

export type LogLevel = "Debug" | "Info" | "Warn" | "Error"
export const logColors = {
	Debug: "blue",
	Info: "green",
	Warn: "orange",
	Error: "red"
}

export type Prop = {
	key: string
	value: string
}

export type LogEntry = {
	timestamp: string
	level: LogLevel
	props: Prop[]
	msg: string
}

export type SortDir = "asc" | "desc";

const formatTimestamp = (ts: string) => {
	const date = new Date(ts)
	return date.toLocaleString()
}

export type FetchMoreArgs = {
	offset: number
	count: number
	query: string
}

export const logsSearchPage = (args: {
	isStreaming: boolean
	fetchMore: (args: FetchMoreArgs) => void
	toggleIsStreaming: () => boolean
}) => {
	const root = document.createElement("div")
	const logEntries: LogEntry[] = []
	const options = document.createElement("div")
	options.style.position = "sticky"
	options.style.top = "0"
	options.style.gap = "10px"
	options.style.backgroundColor = "white"
	options.style.height = "100px"
	options.style.display = "flex"
	const searchBar = document.createElement("textarea")
	const tbody = document.createElement("tbody")
	tbody.style.width = "400px"
	const queryLogs = (query: string) => {
		logEntries.length = 0
		tbody.innerHTML = ""
		last.innerHTML = "Loading..."
		args.fetchMore({
			offset: 0,
			count: 100,
			query
		})
	}
	searchBar.style.height = "100px"
	searchBar.style.resize = "none"
	searchBar.style.flexGrow = "1"
	searchBar.value = getQueryParam("query") || ""
	searchBar.onkeydown = (e) => {
		if (e.key === "Enter" && e.ctrlKey) {
			e.preventDefault()
			queryLogs(searchBar.value)
		}
	}
	options.appendChild(searchBar)
	const searchButton = document.createElement("button")
	searchButton.onclick = () => {
		queryLogs(searchBar.value)
	}
	searchButton.innerHTML = "Search"
	options.appendChild(searchButton)
	const streamButton = document.createElement("button")
	const streamButtonState = (state: boolean) => state ? "Stop<br />Stream" : "Start<br />Stream"
	streamButton.innerHTML = streamButtonState(args.isStreaming)
	streamButton.onclick = () => {
		const isStreaming = args.toggleIsStreaming()
		streamButton.innerHTML = streamButtonState(isStreaming)
	}
	options.appendChild(streamButton)
	root.appendChild(options)
	const table = document.createElement("table")
	table.style.width = "100%"
	const thead = document.createElement("thead")
	thead.style.position = "sticky"
	thead.style.top = "100px"
	thead.style.backgroundColor = "white"
	thead.innerHTML = `
		<tr>
			<th>Timestamp</th>
			<th>Level</th>
			<th>Props</th>
			<th>Message</th>
		</tr>
	`
	table.appendChild(thead)
	table.appendChild(tbody)
	const tableWrapper = document.createElement("div")
	tableWrapper.style.overflow = "auto"
	tableWrapper.appendChild(table)
	root.appendChild(table)
	const last = document.createElement("div")
	last.style.height = "100px"
	last.innerHTML = "Loading..."
	root.appendChild(last)
	let moreRows = true
	const observer = new IntersectionObserver(() => {
		console.log("intersect")
		if (!moreRows) return
		console.log("need to fetch more")
		moreRows = false
		args.fetchMore({
			offset: logEntries.length,
			count: 100,
			query: searchBar.value
		})
	}, {
		root: null,
		rootMargin: "0px",
		threshold: 0.1,
	})
	observer.observe(last)

	const escapeHTML = (str: string) => {
		const div = document.createElement('div');
		div.textContent = str;
		return div.innerHTML;
	};

	return {
		root,
		setIsStreaming: (isStreaming: boolean) => {
			streamButton.innerHTML = streamButtonState(isStreaming)
		},
		onError (err: string) {
			last.innerHTML = err
		},
		addLogEntries: (entries: LogEntry[]) => {	
			if (entries.length === 0) {
				last.innerHTML = "No more rows"
				return
			}
			setTimeout(() => {
				moreRows = true
			}, 500)
			logEntries.push(...entries)
			logEntries.sort((a, b) => b.timestamp.localeCompare(a.timestamp))
			const body = `
				${logEntries.map((r) => {
					// const textNode = document.createTextNode();
					return`
						<tr style="height: 35px">
							<td style="white-space: nowrap; vertical-align: top; text-align: center;">${formatTimestamp(r.timestamp)}</td>
							<td style="color: ${logColors[r.level]}; vertical-align: top; text-align: center;text-align: center;">${r.level}</td>
							<td style="vertical-align: top; text-align: left;">${r.props.map((p) => `${p.key}=${p.value}`).join("<br />")}</td>
							<td style="word-break: break-all; vertical-align: top">${escapeHTML(`${r.msg.slice(0, 700)}${r.msg.length > 700 ? "..." : ""}`)}</td>
						</tr>
					`	
				}).join("")}
			`
			tbody.innerHTML = body
		}
	}
}