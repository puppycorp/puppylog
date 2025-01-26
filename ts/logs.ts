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

export class Logtable {
	public root: HTMLElement
    private table = document.createElement("table")
    private header: HTMLElement
    private body: HTMLElement
    public sortDir: SortDir = "desc"
    private logSearcher: LogSearcher
	private virtual: VirtualTable
    private errorText: HTMLElement

    constructor() {
        this.root = document.createElement('div')

        this.header = document.createElement('head')
        this.header.innerHTML = `<tr><th>Timestamp</th><th>Level</th><th>Props</th><th>Message</th></tr>`
        this.table.appendChild(this.header)
        this.body = document.createElement('tbody') 
        this.table.appendChild(this.body)

		this.logSearcher = new LogSearcher({
            onNewLoglines: this.onNewLoglines.bind(this),
            onClear: () => {},
            onError: (err) => {
                this.errorText.innerHTML = err
            }
        })
        this.virtual = new VirtualTable({
            rowCount: 0,
            rowHeight: 35, 
            drawRow: (start, end) => {
				console.log(`draw start: ${start} end: ${end}`)
                let body = ""
                for (let i = start; i < end; i++) {
                    const r = this.logSearcher.logEntries[i]
                    body += `
                    <tr style="height: 35px">
                        <td style="white-space: nowrap">${formatTimestamp(r.timestamp)}</td>
                        <td style="color: ${logColors[r.level]}">${r.level}</td>
						<td>${r.props.map((p) => p.join("=")).join(", ")}</td>
                        <td style="word-break: break-all"><pre>${r.msg}</pre></td>
                    </tr>
                    `
                }
                this.body.innerHTML = body
                return this.table
            },
			fetchMore: this.fetchMore.bind(this)
        })
        const searchOptions = new LogSearchOptions({
            searcher: this.logSearcher
        })
        this.root.appendChild(searchOptions.root)
        this.errorText = document.createElement('div')
        this.errorText.style.color = "red"
        this.root.appendChild(this.errorText)
        this.root.appendChild(this.virtual.root)

        // this.logSearcher.search({
        //     count: 100
        // })
        this.logSearcher.stream()

        window.addEventListener("scroll", (e) => {
            console.log("scroll", e)
        })
    }

	private onNewLoglines() {
		console.log("onNewLoglines")
		this.virtual.setRowCount(this.logSearcher.logEntries.length)
	}

	private fetchMore() {
		if (!this.logSearcher) return
		console.log("fetchMore")
		this.logSearcher.fetchMore()
	}

    public sort(dir: SortDir) {
        this.sortDir = dir
    }
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
	root.appendChild(table)
	// requestAnimationFrame(() => {
	// 	queryLogs(searchBar.value)
	// })

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

	return {
		root,
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
			console.log("logEntries", logEntries)
			const body = `
				${logEntries.map((r) => `
				<tr style="height: 35px">
					<td style="white-space: nowrap; vertical-align: top"><pre>${formatTimestamp(r.timestamp)}</pre></td>
					<td style="color: ${logColors[r.level]}; vertical-align: top"><pre>${r.level}</pre></td>
					<td style="vertical-align: top"><pre>${r.props.map((p) => `${p.key}=${p.value}`)}</pre></td>
					<td style="word-break: break-all; vertical-align: top"><pre>${r.msg}</pre></td>
				</tr>
				`).join("")}
			`
			tbody.innerHTML = body
		}
	}
}