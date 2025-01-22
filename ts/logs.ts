import { getQueryParam, setQueryParam } from "./utility"
import { VirtualTable } from "./virtual-table"

export type LogLevel = "Debug" | "Info" | "Warn" | "Error"
export const logColors = {
	Debug: "blue",
	Info: "green",
	Warn: "orange",
	Error: "red"
}

export type LogEntry = {
    timestamp: string
	level: LogLevel
    props: string[][]
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
                        <td style="word-break: break-all">${r.msg}</td>
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

export class LogSearchOptions {
    public root: HTMLElement
    private input: HTMLTextAreaElement
    private button: HTMLButtonElement
    // private startDate: HTMLInputElement
    // private endDate: HTMLInputElement
    private searcher: LogSearcher

    constructor(args: {
        searcher: LogSearcher
    }) {
        this.root = document.createElement('div')
		this.root.style.display = "flex"
		this.root.style.gap = "10px"
        this.input = document.createElement('textarea')
        this.input.value = getQueryParam("query") || ""
        this.input.rows = 4
		this.input.style.width = "400px"
		this.input.onkeydown = (e) => {
			console.log("key: ", e.key, " shift: ", e.shiftKey)
			if (e.key === "Enter" && !e.shiftKey) {
				console.log("preventing default")
				e.preventDefault()
                this.searcher.setQuery(this.input.value)
			}
		}
				
        this.button = document.createElement('button')
        this.button.onclick = () => {
            this.searcher.setQuery(this.input.value)
        }
        this.button.innerHTML = "Search"
        this.root.appendChild(this.input)
        this.root.appendChild(this.button)
        // this.startDate = document.createElement('input')
        // this.startDate.type = "date"
        // this.root.appendChild(this.startDate)
        // this.endDate = document.createElement('input')
        // this.endDate.type = "date"
        // this.root.appendChild(this.endDate)
        this.searcher = args.searcher
    }

    public getQuery(): string {
        return this.input.value
    }
}

export class LogSearcher {
    private logEventSource?: EventSource
	private sortDir: SortDir = "desc"
    private onClear: () => void
    private onNewLoglines: () => void
    private onError: (err: string) => void
    public logEntries: LogEntry[] = []
	public firstDate?: string
	public lastDate?: string
    private query: string = ""
    private offset: number = 0
    private count: number = 100
    private alreadyFetched: boolean = false

    public constructor(args: {
        onClear: () => void
        onNewLoglines: () => void
        onError: (err: string) => void
    }) {
        this.onClear = args.onClear
        this.onNewLoglines = args.onNewLoglines
        this.onError = args.onError
        this.query = getQueryParam("query") || ""
    }

	private buildQuery(stream?: boolean) {
		const offsetInMinutes = new Date().getTimezoneOffset();
        const offsetInHours = -offsetInMinutes / 60;
		const urlQuery = new URLSearchParams()
        urlQuery.append("timezone", offsetInHours.toString())
        if (this.query) {
            urlQuery.append("query", this.query)
        }
		if (!stream) {
        	urlQuery.append("count", this.count.toString())
        	urlQuery.append("offset", this.offset.toString())
		}
		return urlQuery
	}

    public stream() {
		const url = new URL("http://localhost:3337/api/logs/stream")
		url.search = this.buildQuery(true).toString()
        this.createEventSource(url.toString())
    }

    public setQuery(query: string) {
        this.query = query
        this.offset = 0
        this.alreadyFetched = false
        setQueryParam("query", query)
        this.logEntries = []
        this.fetchMore()
		this.stream()
    }

    public fetchMore() {
        if (this.alreadyFetched) return
        this.alreadyFetched = true
        const url = new URL("http://localhost:3337/api/logs")
        url.search = this.buildQuery().toString()
        fetch(url.toString()).then(async (res) => {
            if (res.status === 400) {
                const err = await res.json()
                this.onError(err.error)
                console.log("res", res)
                throw new Error("Failed to fetch logs")
            }
            this.onError("")
            return res.json()
        }).then((data) => {
            this.logEntries.push(...data)
            this.handleSort()
            this.onNewLoglines()
            this.offset += this.count
            if (data.length >= this.count) {
                this.alreadyFetched = false
            }
        }).catch((err) => {
            console.error("error", err)
        })
    }

    private createEventSource(url: string) {
        if (this.logEventSource) {
            this.logEventSource.close()
            this.onClear()
        }

        this.logEventSource = new EventSource(url)
        this.logEventSource.onmessage = (e) => {
            console.log("Got message", e.data)
            this.logEntries.push(JSON.parse(e.data))
			this.handleSort()
			this.onNewLoglines()
        }
        this.logEventSource.onerror = (err) => {
            console.error("error", err)
            this.logEventSource?.close()
        }
    }

	private handleSort() {
		if (this.logEntries.length === 0) return
		if (this.sortDir === "asc") this.logEntries.sort((a, b) => a.timestamp.localeCompare(b.timestamp))
		else this.logEntries.sort((a, b) => b.timestamp.localeCompare(a.timestamp))
		this.firstDate = this.logEntries[0].timestamp
		this.lastDate = this.logEntries[this.logEntries.length - 1].timestamp
	}
}