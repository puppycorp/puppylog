
export type LogLevel = "Debug" | "Info" | "Warn" | "Error"
export const logColors = {
	Debug: "blue",
	Info: "green",
	Warn: "orange",
	Error: "red"
}

export type LogRow = {
    timestamp: string
	level: "debug" | "info" | "warn" | "error"
    props: string[]
    msg: string
}

export type SortDir = "asc" | "desc";

export class Logtable {
	public root: HTMLElement 
    private header: HTMLElement
    private body: HTMLElement
    public sortDir: SortDir = "desc"
    private logSearcher: LogSearcher

    constructor() {
        this.root = document.createElement('table')
        this.header = document.createElement('tr')
        this.header.innerHTML = `<th>Timestamp</th><th>Level</th><th>message</th>`
        this.root.appendChild(this.header)
        this.body = document.createElement('tbody') 
        this.root.appendChild(this.body)

        this.logSearcher = new LogSearcher({
            onNewLoglines: (rows) => {
                this.addRows(rows)
            },
            onClear: () => {
                this.body.innerHTML
            }
        })

        this.logSearcher.search({})
        this.logSearcher.stream()
    }

    public addRows(rows: LogRow[]) {
        console.log("Adding rows", rows)
        for (const r of rows) {
            const row = document.createElement('tr')
            row.innerHTML = `<td>${r.timestamp}</td><td style="color: ${logColors[r.level]}">${r.level}</td><td>${r.msg}</td>`

            if (this.sortDir === "asc") {
                this.body.prepend(row)
            }
            else {
                this.body.appendChild(row)
            }
        }
    }

    public sort(dir: SortDir) {
        this.sortDir = dir
    }
}

export class LogSearch {
    public root: HTMLElement
    private input: HTMLInputElement
    private button: HTMLButtonElement
    private startDate: HTMLInputElement
    private endDate: HTMLInputElement

    constructor() {
        this.root = document.createElement('div')
        this.input = document.createElement('input')
        this.input.type = "text"
        this.button = document.createElement('button')
        this.button.innerHTML = "Search"
        this.root.appendChild(this.input)
        this.root.appendChild(this.button)
        this.startDate = document.createElement('input')
        this.startDate.type = "date"
        this.root.appendChild(this.startDate)
        this.endDate = document.createElement('input')
        this.endDate.type = "date"
        this.root.appendChild(this.endDate)
    }

    public getQuery(): string {
        return this.input.value
    }
}

export class LogSearcher {
    private logEventSource?: EventSource
    private onClear: () => void
    private onNewLoglines: (rows: LogRow[]) => void

    public constructor(args: {
        onClear: () => void
        onNewLoglines: (rows: LogRow[]) => void
    }) {
        this.onClear = args.onClear
        this.onNewLoglines = args.onNewLoglines
    }

    public stream() {
        this.createEventSource("http://localhost:3000/api/logs/stream")
    }

    public search(args: {
        startDate?: string
        endDate?: string
        search?: string[]
        count?: number
    }) {
        const query = new URLSearchParams()
        if (args.startDate) {
            query.append("startDate", args.startDate)
        }
        if (args.endDate) {
            query.append("endDate", args.endDate)
        }
        if (args.search) {
            for (const s of args.search) {
                query.append("search", s)
            }
        }
        if (args.count) {
            query.append("count", args.count.toString())
        }

        const url = new URL("http://localhost:3000/api/logs")
        url.search = query.toString()

        fetch(url.toString()).then((res) => res.json()).then((data) => {
            this.onNewLoglines(data)
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
            this.onNewLoglines([JSON.parse(e.data)])
        }
        this.logEventSource.onerror = (err) => {
            console.error("error", err)
            this.logEventSource?.close()
        }
    }
}