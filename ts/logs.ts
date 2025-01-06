
export type LogRow = {
    timestamp: string
    tags: string[]
    message: string
}

export type SortDir = "asc" | "desc";

export class Logtable {
	public root: HTMLElement 
    private header: HTMLElement
    private body: HTMLElement
    public sortDir: SortDir = "desc"

    constructor() {
        this.root = document.createElement('table')
        this.header = document.createElement('tr')
        this.header.innerHTML = `<th>Timestamp</th><th>message</th>`
        this.root.appendChild(this.header)
        this.body = document.createElement('tbody') 
        this.root.appendChild(this.body)

        fetch("http://localhost:3000/api/logs?count=10").then((res) => res.json()).then((data) => {
            this.addRows(data)
        })
    }

    public addRows(rows: LogRow[]) {
        console.log("Adding rows", rows)
        for (const r of rows) {
            const row = document.createElement('tr')
            row.innerHTML = `<td>${r.timestamp}</td><td>${r.message}</td>`

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

export class Logsearcher {
    
}