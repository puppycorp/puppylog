
export class VirtualTable {
    public root: HTMLElement
    private container: HTMLElement
    private table: HTMLTableElement
    private rowHeight: number
    public rowCount: number
    private bufferSize = 10
	private needMoreRows = false
    private drawRow: (start: number, end: number) => HTMLElement
	private fetchMore?: () => void

    public constructor(args: {
        rowHeight: number
        rowCount: number
        drawRow: (start: number, end: number) => HTMLElement
		fetchMore?: () => void
    }) {
        this.drawRow = args.drawRow
		this.fetchMore = args.fetchMore
        this.rowHeight = args.rowHeight
        this.rowCount = args.rowCount
        this.root = document.createElement("div")
        this.root.style.height = "800px"
        this.root.style.width = "100%"
        this.root.style.overflow = "auto"
        this.container = document.createElement("div")
        // this.container.style.overflow = "scroll"
        this.container.style.position = "relative"
        this.root.appendChild(this.container)
        this.container.style.height = `${args.rowHeight * args.rowCount}px`
        this.container.style.width = "100%"
		this.container.style.marginTop = "50px"
		this.container.style.marginBottom = "50px"
        //this.container.style.border = "1px solid black"
        this.container.innerHTML = "Virtual Table"

        this.table = document.createElement("table")
        this.container.appendChild(this.table)

        this.root.addEventListener("scroll", (e) => {
            this.onScroll(e)
        })
		const handleObserver = (entries: IntersectionObserverEntry[]) => {
			console.log("Intersection observer", entries);
		}
		const observer = new IntersectionObserver(handleObserver, {
			root: this.root,
			rootMargin: '0px',
			threshold: 0.1
		});
		setTimeout(() => {
			if (this.fetchMore) this.fetchMore()
		})
    }

    private onScroll(e: Event) {
        requestAnimationFrame(() => this.updateVisibleRows());
    }

    public updateVisibleRows() {
        const scrollTop = this.root.scrollTop;
        const containerHeight = this.root.clientHeight;
        //console.log("containerHeight", containerHeight);
        //console.log("o", this.root.scrollHeight)

        // Calculate visible range
        const startIndex = Math.max(0, Math.floor(scrollTop / this.rowHeight) - this.bufferSize);
        const endIndex = Math.min(
            this.rowCount,
            Math.ceil((scrollTop + containerHeight) / this.rowHeight) + this.bufferSize
        );

        //console.log("Visible range", startIndex, endIndex);
        const content = this.drawRow(startIndex, endIndex);
        content.style.position = "absolute";
        content.style.top = `${startIndex * this.rowHeight}px`;
        this.container.innerHTML = "";
        this.container.appendChild(content);

		//console.log("height: " + this.root.style.height)
		const rootRect = this.root.getBoundingClientRect()
		//console.log("rootRect", rootRect)
		const containerRect = this.container.getBoundingClientRect()
		//console.log("containerRect", containerRect)

		// const maxHeight = 

		const rootBottom = rootRect.bottom
		const containerBottom = containerRect.bottom

		requestAnimationFrame(() => {
			if (containerBottom < rootBottom + 3*this.rowHeight) {
				console.log("need more rows")
				if (this.needMoreRows) return
				this.needMoreRows = true
				if (this.fetchMore) this.fetchMore()
			}
		})
    }

	public setRowCount(rowCount: number) {
		const scrollTop = this.root.scrollTop;
		const oldStartIndex = Math.floor(scrollTop / this.rowHeight);
		
		this.rowCount = rowCount;
		this.container.style.height = `${this.rowHeight * rowCount + this.rowHeight * 3}px`;
		
		// Restore scroll to keep same rows visible
		this.root.scrollTop = oldStartIndex * this.rowHeight;
		
		this.updateVisibleRows();
		this.needMoreRows = false;
	}
}