
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
        this.header.innerHTML = `<th>Timestamp</th><th>tags</th><th>message</th>`
        this.root.appendChild(this.header)
        this.body = document.createElement('tbody')
        this.body.innerHTML = `<tr><td>2020-01-01 12:00:00</td><td>APP</td><td>hello world</td></tr>`
        this.root.appendChild(this.body)
    }

    public addRows(rows: LogRow[]) {
        for (const r of rows) {
            const row = document.createElement('tr')
            row.innerHTML = `<td>${r.timestamp}</td><td>${r.tags.join(', ')}</td><td>${r.message}</td>`

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

    constructor() {
        this.root = document.createElement('div')
        this.input = document.createElement('input')
        this.input.type = "text"
        this.button = document.createElement('button')
        this.button.innerHTML = "Search"
        this.root.appendChild(this.input)
        this.root.appendChild(this.button)
    }

    public getQuery(): string {
        return this.input.value
    }
}