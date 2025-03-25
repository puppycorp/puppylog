import { Prop } from "./logs"
import { formatBytes, formatNumber, formatTimestamp } from "./utility"
type Segment = {
	id: number
	firstTimestamp: string
	lastTimestamp: string
	originalSize: number
	compressedSize: number
	logsCount: number
}
export const segmentsPage = async (root: HTMLElement) => {
	const res = await fetch("/api/v1/segments").then(res => res.json()) as Segment[]
	const totalSegments = res.length
	const totalOriginalSize = res.reduce((sum, seg) => sum + seg.originalSize, 0)
	const totalCompressedSize = res.reduce((sum, seg) => sum + seg.compressedSize, 0)
	const totalLogsCount = res.reduce((sum, seg) => sum + seg.logsCount, 0)
	const compressRatio = totalCompressedSize / totalOriginalSize * 100
	root.innerHTML = `
		<div class="page-header">
			<h1 style="flex-grow: 1">Segments</h1>
			<div class="summary">
				<div><strong>Total segments:</strong> ${formatNumber(totalSegments)}</div>
				<div><strong>Total original size:</strong> ${formatBytes(totalOriginalSize)}</div>
				<div><strong>Total compressed size:</strong> ${formatBytes(totalCompressedSize)}</div>
				<div><strong>Total logs count:</strong> ${formatNumber(totalLogsCount)}</div>
				<div><strong>Compression ratio:</strong> ${compressRatio.toFixed(2)}%</div>
			</div>
		</div>
		<div style="display: flex; flex-wrap: wrap; gap: 10px; margin: 10px">
			${res.map(segment => `
				<div class="list-row">
					<div class="table-cell"><strong>Segment ID:</strong> <a href="/segment/${segment.id}">${segment.id}</a></div>
					<div class="table-cell"><strong>First timestamp:</strong> ${segment.firstTimestamp}</div>
					<div class="table-cell"><strong>Last timestamp:</strong> ${segment.lastTimestamp}</div>
					<div class="table-cell"><strong>Original size:</strong> ${formatBytes(segment.originalSize)}</div>
					<div class="table-cell"><strong>Compressed size:</strong> ${formatBytes(segment.compressedSize)}</div>
					<div class="table-cell"><strong>Logs count:</strong> ${formatNumber(segment.logsCount)}</div>
					<div class="table-cell"><strong>Compression ratio:</strong> ${((segment.compressedSize / segment.originalSize) * 100).toFixed(2)}%</div>
				</div>
			`).join("")}
		</div>
	`
}


export const segmentPage = async (root: HTMLElement, segmentId: number) => {
	const segment = await fetch(`/api/v1/segment/${segmentId}`).then(res => res.json()) as Segment
	const props = await fetch(`/api/v1/segment/${segmentId}/props`).then(res => res.json()) as Prop[]

	const totalOriginalSize = segment.originalSize
	const totalCompressedSize = segment.compressedSize
	const totalLogsCount = segment.logsCount
	const compressRatio = totalCompressedSize / totalOriginalSize * 100
	root.innerHTML = `
		<div class="page-header">
			<h1 style="flex-grow: 1">Segment ${segmentId}</h1>
			<div class="summary">
				<div><strong>First timestamp:</strong> ${formatTimestamp(segment.firstTimestamp)}</div>
				<div><strong>Last timestamp:</strong> ${formatTimestamp(segment.lastTimestamp)}</div>
				<div><strong>Total original size:</strong> ${formatBytes(totalOriginalSize)}</div>
				<div><strong>Total compressed size:</strong> ${formatBytes(totalCompressedSize)}</div>
				<div><strong>Total logs count:</strong> ${formatNumber(totalLogsCount)}</div>
				<div><strong>Compression ratio:</strong> ${compressRatio.toFixed(2)}%</div>
			</div>
		</div>
		<div style="display: flex; flex-wrap: wrap; gap: 10px; margin: 10px">
			${props.map(prop => `
				<div class="list-row">
					<div class="table-cell"><strong>Key:</strong> ${prop.key}</div>
					<div class="table-cell"><strong>Value:</strong> ${prop.value}</div>
				</div>
			`).join("")}
		</div>
	`
}