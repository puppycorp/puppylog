import { Prop } from "./logs"
import {
	Collapsible,
	Container,
	InfiniteScroll,
	KeyValueTable,
	WrapList,
} from "./ui"
import { Navbar } from "./navbar"
import { formatBytes, formatNumber, formatTimestamp } from "./utility"
type Segment = {
	id: number
	firstTimestamp: string
	lastTimestamp: string
	originalSize: number
	compressedSize: number
	logsCount: number
}

type SegementsMetadata = {
	segmentCount: number
	originalSize: number
	compressedSize: number
	logsCount: number
	averageLogsPerSegment: number
	averageSegmentSize: number
}

const fetchSegments = async (end: Date) => {
	const url = new URL("/api/segments", window.location.origin)
	url.searchParams.set("end", end.toISOString())
	const res = (await fetch(url.toString()).then((res) =>
		res.json(),
	)) as Segment[]
	return res
}

export const segmentsPage = async (root: Container, navbar?: Navbar) => {
	const segementsMetadata = (await fetch("/api/segment/metadata").then(
		(res) => res.json(),
	)) as SegementsMetadata
	const compressionRatio =
		(segementsMetadata.compressedSize / segementsMetadata.originalSize) *
		100
	const averageCompressedLogSize =
		segementsMetadata.compressedSize / segementsMetadata.logsCount
	const averageOriginalLogSize =
		segementsMetadata.originalSize / segementsMetadata.logsCount
	const metadata = new KeyValueTable([
		{
			key: "Total segments",
			value: formatNumber(segementsMetadata.segmentCount),
		},
		{
			key: "Total original size",
			value: formatBytes(segementsMetadata.originalSize),
		},
		{
			key: "Total compressed size",
			value: formatBytes(segementsMetadata.compressedSize),
		},
		{
			key: "Total logs count",
			value: formatNumber(segementsMetadata.logsCount),
		},
		{ key: "Compression ratio", value: compressionRatio.toFixed(2) + "%" },
		{
			key: "Average compressed log size",
			value: formatBytes(averageCompressedLogSize),
		},
		{
			key: "Average original log size",
			value: formatBytes(averageOriginalLogSize),
		},
		{
			key: "Average logs per segment",
			value: formatNumber(
				segementsMetadata.logsCount / segementsMetadata.segmentCount,
			),
		},
		{
			key: "Average segment size",
			value: formatBytes(
				segementsMetadata.originalSize / segementsMetadata.segmentCount,
			),
		},
	])
	metadata.root.style.whiteSpace = "nowrap"
	const metadataCollapsible = new Collapsible({
		buttonText: "Metadata",
		content: metadata,
	})
	const nav = navbar ?? new Navbar({ right: [metadataCollapsible] })
	if (navbar) {
		nav.setRight([metadataCollapsible])
	} else {
		root.add(nav)
	}

	const segmentList = new WrapList()
	const infiniteScroll = new InfiniteScroll({
		container: segmentList,
	})
	root.add(infiniteScroll)
	let endDate = new Date()
	infiniteScroll.onLoadMore = async () => {
		console.log("loadMore")
		const segments = await fetchSegments(endDate)
		endDate = new Date(segments[segments.length - 1].lastTimestamp)
		for (const segment of segments) {
			const table = new KeyValueTable([
				{
					key: "Segment ID",
					value: segment.id.toString(),
					href: `/segment/${segment.id}`,
				},
				{
					key: "First timestamp",
					value: formatTimestamp(segment.firstTimestamp),
				},
				{
					key: "Last timestamp",
					value: formatTimestamp(segment.lastTimestamp),
				},
				{
					key: "Original size",
					value: formatBytes(segment.originalSize),
				},
				{
					key: "Compressed size",
					value: formatBytes(segment.compressedSize),
				},
				{ key: "Logs count", value: formatNumber(segment.logsCount) },
				{
					key: "Compression ratio",
					value:
						(
							(segment.compressedSize / segment.originalSize) *
							100
						).toFixed(2) + "%",
				},
			])
			segmentList.add(table)
		}
	}
}

export const segmentPage = async (
	root: HTMLElement,
	segmentId: number,
	navbar?: Navbar,
) => {
	const segment = (await fetch(`/api/v1/segment/${segmentId}`).then((res) =>
		res.json(),
	)) as Segment
	const props = (await fetch(`/api/v1/segment/${segmentId}/props`).then(
		(res) => res.json(),
	)) as Prop[]

	const totalOriginalSize = segment.originalSize
	const totalCompressedSize = segment.compressedSize
	const totalLogsCount = segment.logsCount
	const compressRatio = (totalCompressedSize / totalOriginalSize) * 100
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
			${props
				.map(
					(prop) => `
				<div class="list-row">
					<div class="table-cell"><strong>Key:</strong> ${prop.key}</div>
					<div class="table-cell"><strong>Value:</strong> ${prop.value}</div>
				</div>
			`,
				)
				.join("")}
                </div>
        `
	if (navbar) root.prepend(navbar.root)
}
