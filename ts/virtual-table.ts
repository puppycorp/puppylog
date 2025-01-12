
export class VirtualTable {
    public root: HTMLElement
    private container: HTMLElement
    private table: HTMLTableElement
    private rowHeight: number
    public rowCount: number
    private bufferSize = 10
    private drawRow: (start: number, end: number) => HTMLElement

    public constructor(args: {
        rowHeight: number
        rowCount: number
        drawRow: (start: number, end: number) => HTMLElement
    }) {
        this.drawRow = args.drawRow
        this.rowHeight = args.rowHeight
        this.rowCount = args.rowCount
        this.root = document.createElement("div")
        this.root.style.height = "500px"
        this.root.style.width = "800px"
        this.root.style.overflow = "scroll"
        this.container = document.createElement("div")
        this.container.style.overflow = "scroll"
        this.container.style.position = "relative"
        this.root.appendChild(this.container)
        this.container.style.height = `${args.rowHeight * args.rowCount}px`
        this.container.style.width = "100%"
        this.container.style.border = "1px solid black"
        this.container.innerHTML = "Virtual Table"

        this.table = document.createElement("table")
        this.container.appendChild(this.table)

        this.root.addEventListener("scroll", (e) => {
            this.onScroll(e)
        })
        this.updateVisibleRows()
    }

    private onScroll(e: Event) {
        requestAnimationFrame(() => this.updateVisibleRows());
    }

    public updateVisibleRows() {
        const scrollTop = this.root.scrollTop;
        const containerHeight = this.root.clientHeight;
        console.log("containerHeight", containerHeight);
        console.log("o", this.root.scrollHeight)

        // Calculate visible range
        const startIndex = Math.max(0, Math.floor(scrollTop / this.rowHeight) - this.bufferSize);
        const endIndex = Math.min(
            this.rowCount,
            Math.ceil((scrollTop + containerHeight) / this.rowHeight) + this.bufferSize
        );

        console.log("Visible range", startIndex, endIndex);
        const content = this.drawRow(startIndex, endIndex);
        content.style.position = "absolute";
        content.style.top = `${startIndex * this.rowHeight}px`;
        this.container.innerHTML = "";
        this.container.appendChild(content);
    }

    public setRowCount(rowCount: number) {
        this.rowCount = rowCount;
        this.container.style.height = `${this.rowHeight * rowCount}px`;
        this.updateVisibleRows();
    }
}