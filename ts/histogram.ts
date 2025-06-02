export type HistogramItem = {
    timestamp: string
    count: number
}

export class Histogram {
    public readonly root: HTMLDivElement
    private canvas: HTMLCanvasElement
    private ctx: CanvasRenderingContext2D
    private data: HistogramItem[] = []
    private zoom = 1

    constructor() {
        this.root = document.createElement('div')
        this.root.style.overflowX = 'auto'
        this.canvas = document.createElement('canvas')
        this.canvas.height = 200
        this.canvas.width = 600
        this.root.appendChild(this.canvas)
        const ctx = this.canvas.getContext('2d')
        if (!ctx) throw new Error('canvas 2d context not available')
        this.ctx = ctx

        this.root.addEventListener('wheel', (e: WheelEvent) => {
            e.preventDefault()
            const delta = e.deltaY < 0 ? 1.1 : 0.9
            this.setZoom(this.zoom * delta)
        })
    }

    public clear() {
        this.data = []
        this.draw()
    }

    public add(item: HistogramItem) {
        this.data.push(item)
        this.draw()
    }

    private setZoom(z: number) {
        this.zoom = Math.min(5, Math.max(0.5, z))
        this.draw()
    }

    private draw() {
        const ctx = this.ctx
        ctx.clearRect(0, 0, this.canvas.width, this.canvas.height)
        if (this.data.length === 0) return
        const max = Math.max(...this.data.map(d => d.count))
        const barWidth = 10 * this.zoom
        const width = Math.max(this.canvas.parentElement?.clientWidth || 600, barWidth * this.data.length)
        this.canvas.width = width
        for (let i = 0; i < this.data.length; i++) {
            const item = this.data[i]
            const h = (item.count / max) * this.canvas.height
            ctx.fillStyle = '#3B82F6'
            ctx.fillRect(i * barWidth, this.canvas.height - h, barWidth - 1, h)
        }
    }
}
