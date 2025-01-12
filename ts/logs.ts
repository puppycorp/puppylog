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
    props: string[]
    msg: string
}

export type SortDir = "asc" | "desc";

export class Logtable {
	public root: HTMLElement
    private table = document.createElement("table")
    private header: HTMLElement
    private body: HTMLElement
    public sortDir: SortDir = "desc"
    private logSearcher: LogSearcher

    constructor() {
        this.root = document.createElement('div')

        this.header = document.createElement('head')
        this.header.innerHTML = `<tr><th>Timestamp</th><th>Level</th><th>message</th></tr>`
        this.table.appendChild(this.header)
        this.body = document.createElement('tbody') 
        this.table.appendChild(this.body)


        const virtual = new VirtualTable({
            rowCount: 0,
            rowHeight: 35, 
            drawRow: (start, end) => {
                let body = ""
                for (let i = start; i < end; i++) {
                    const r = this.logSearcher.logEntries[i]
                    body += `
                    <tr style="height: 35px">
                        <td>${r.timestamp}</td>
                        <td style="color: ${logColors[r.level]}">${r.level}</td>
                        <td>${i} - ${r.msg}</td>
                    </tr>
                    `
                }
                this.body.innerHTML = body
                return this.table
            }
        })

        this.logSearcher = new LogSearcher({
            onNewLoglines: () => {
                // this.rows.push(...rows)
                // this.rows.sort((a, b) => {
                //     if (this.sortDir === "asc") {
                //         return a.timestamp.localeCompare(b.timestamp)
                //     }
                //     else {
                //         return b.timestamp.localeCompare(a.timestamp)
                //     }
                // })
                virtual.setRowCount(this.logSearcher.logEntries.length)
            },
            onClear: () => {
                this.body.innerHTML
            }
        })

        const searchOptions = new LogSearchOptions({
            searcher: this.logSearcher
        })
        this.root.appendChild(searchOptions.root)
        this.root.appendChild(virtual.root)

        this.logSearcher.search({
            count: 100000
        })
        this.logSearcher.stream()

        window.addEventListener("scroll", (e) => {
            console.log("scroll", e)
        })
    }

    // public addRows(rows: LogRow[]) {
    //     console.log("Adding rows", rows)
    //     for (const r of rows) {
    //         const row = document.createElement('tr')
    //         row.innerHTML = `<td>${r.timestamp}</td><td style="color: ${logColors[r.level]}">${r.level}</td><td>${r.msg}</td>`

    //         if (this.sortDir === "asc") {
    //             this.body.prepend(row)
    //         }
    //         else {
    //             this.body.appendChild(row)
    //         }
    //     }
    // }

    public sort(dir: SortDir) {
        this.sortDir = dir
    }
}

export class LogSearchOptions {
    public root: HTMLElement
    private input: HTMLInputElement
    private button: HTMLButtonElement
    private startDate: HTMLInputElement
    private endDate: HTMLInputElement
    private searcher: LogSearcher

    constructor(args: {
        searcher: LogSearcher
    }) {
        this.root = document.createElement('div')
        this.input = document.createElement('input')
        this.input.type = "text"
        this.button = document.createElement('button')
        this.button.onclick = () => {
            this.searcher.search({
                search: [this.input.value],
            })
        }
        this.button.innerHTML = "Search"
        this.root.appendChild(this.input)
        this.root.appendChild(this.button)
        this.startDate = document.createElement('input')
        this.startDate.type = "date"
        this.root.appendChild(this.startDate)
        this.endDate = document.createElement('input')
        this.endDate.type = "date"
        this.root.appendChild(this.endDate)
        this.searcher = args.searcher
    }

    public getQuery(): string {
        return this.input.value
    }
}

export class LogSearcher {
    private logEventSource?: EventSource
    private onClear: () => void
    private onNewLoglines: () => void
    public logEntries: LogEntry[] = []

    public constructor(args: {
        onClear: () => void
        onNewLoglines: () => void
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
        order?: "asc" | "desc"
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

                this.logEntries = this.logEntries.filter((l) => {
                    return l.msg.includes(s)
                })
            }
        }
        if (args.count) {
            query.append("count", args.count.toString())
        }

        const url = new URL("http://localhost:3000/api/logs")
        url.search = query.toString()
        this.onNewLoglines()

        fetch(url.toString()).then((res) => res.json()).then((data) => {
            this.logEntries.push(...data)
            if (args.order === "asc") {
                this.logEntries.sort((a, b) => a.timestamp.localeCompare(b.timestamp))
            } else {
                this.logEntries.sort((a, b) => b.timestamp.localeCompare(a.timestamp))
            }
            this.onNewLoglines()
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
        }
        this.logEventSource.onerror = (err) => {
            console.error("error", err)
            this.logEventSource?.close()
        }
    }
}