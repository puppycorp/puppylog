import { Prop } from "./logs"
import {
	Button,
	Collapsible,
	Container,
	InfiniteScroll,
	KeyValueTable,
	WrapList,
} from "./ui"
import { Navbar } from "./navbar"
import { formatBytes, formatNumber, formatTimestamp } from "./utility"
const SEGMENTS_PAGE_SIZE = 50

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

const parseDateInput = (value: string): Date | null => {
	if (!value) return null
	const date = new Date(value)
	return Number.isNaN(date.getTime()) ? null : date
}

const formatDateInputValue = (date: Date | null) => {
	if (!date) return ""
	const pad = (n: number) => String(n).padStart(2, "0")
	const year = date.getFullYear()
	const month = pad(date.getMonth() + 1)
	const day = pad(date.getDate())
	const hours = pad(date.getHours())
	const minutes = pad(date.getMinutes())
	return `${year}-${month}-${day}T${hours}:${minutes}`
}

const fetchSegments = async (args: {
	end: Date
	start?: Date | null
	count?: number
}) => {
	const url = new URL("/api/segments", window.location.origin)
	url.searchParams.set("end", args.end.toISOString())
	url.searchParams.set("sort", "desc")
	const count = args.count ?? SEGMENTS_PAGE_SIZE
	url.searchParams.set("count", count.toString())
	if (args.start) url.searchParams.set("start", args.start.toISOString())
	const res = (await fetch(url.toString()).then((res) =>
		res.json(),
	)) as Segment[]
	return res
}

export const segmentsPage = async (root: Container) => {
	root.root.innerHTML = ""
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
	const navbar = new Navbar({ right: [metadataCollapsible] })
	root.add(navbar)

	const filtersPanel = document.createElement("div")
	filtersPanel.style.display = "flex"
	filtersPanel.style.flexWrap = "wrap"
	filtersPanel.style.gap = "12px"
	filtersPanel.style.alignItems = "flex-end"
	filtersPanel.style.padding = "0 16px"
	filtersPanel.style.marginBottom = "8px"

	const createLabeledInput = (labelText: string) => {
		const wrapper = document.createElement("label")
		wrapper.style.display = "flex"
		wrapper.style.flexDirection = "column"
		wrapper.style.fontSize = "12px"
		const label = document.createElement("span")
		label.textContent = labelText
		label.style.marginBottom = "4px"
		const input = document.createElement("input")
		input.type = "datetime-local"
		input.style.padding = "6px 8px"
		input.style.border = "1px solid #d1d5db"
		input.style.borderRadius = "4px"
		wrapper.append(label, input)
		return { wrapper, input }
	}

	const startInput = createLabeledInput("Start time")
	const endInput = createLabeledInput("End time")
	const applyFiltersButton = new Button({ text: "Apply" })
	const clearFiltersButton = new Button({ text: "Clear" })
	const filterStatus = document.createElement("div")
	filterStatus.style.minHeight = "18px"
	filterStatus.style.fontSize = "12px"
	filterStatus.style.color = "#6b7280"
	filterStatus.style.flexBasis = "100%"

	filtersPanel.append(
		startInput.wrapper,
		endInput.wrapper,
		applyFiltersButton.root,
		clearFiltersButton.root,
		filterStatus,
	)
	root.root.appendChild(filtersPanel)

	const segmentList = new WrapList()
	const infiniteScroll = new InfiniteScroll({
		container: segmentList,
	})
	root.add(infiniteScroll)

	let filterStart: Date | null = null
	let filterEnd: Date | null = null
	let cursorEnd: Date | null = new Date()
	let isLoadingSegments = false
	let segmentsExhausted = false

	const setFilterStatus = (
		message: string,
		type: "info" | "error" | "idle",
	) => {
		filterStatus.textContent = message
		if (type === "error") filterStatus.style.color = "#b91c1c"
		else if (type === "info") filterStatus.style.color = "#047857"
		else filterStatus.style.color = "#6b7280"
	}

	const renderSegmentCard = (segment: Segment) => {
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

	const resetSegments = () => {
		segmentList.root.innerHTML = ""
		cursorEnd = filterEnd ? new Date(filterEnd) : new Date()
		segmentsExhausted = false
	}

	const loadMoreSegments = async (initial = false) => {
		if (segmentsExhausted || isLoadingSegments || !cursorEnd) return
		isLoadingSegments = true
		if (initial) setFilterStatus("Loading segments…", "idle")
		try {
			const segments = await fetchSegments({
				end: cursorEnd,
				start: filterStart,
			})
			if (segments.length === 0 && initial) {
				setFilterStatus(
					"No segments match the current filters.",
					"idle",
				)
				segmentsExhausted = true
			} else {
				setFilterStatus("", "idle")
				for (const segment of segments) renderSegmentCard(segment)
				if (segments.length > 0) {
					const last = segments[segments.length - 1]
					cursorEnd = new Date(last.lastTimestamp)
				}
				if (segments.length < SEGMENTS_PAGE_SIZE) {
					segmentsExhausted = true
				}
			}
		} catch (error) {
			setFilterStatus(
				error instanceof Error
					? error.message || "Failed to load segments."
					: "Failed to load segments.",
				"error",
			)
		} finally {
			isLoadingSegments = false
		}
	}

	infiniteScroll.onLoadMore = async () => {
		if (!segmentsExhausted) await loadMoreSegments()
	}

	const applyFilters = async () => {
		const newStart = parseDateInput(startInput.input.value)
		const newEnd = parseDateInput(endInput.input.value)
		if (newStart && newEnd && newStart > newEnd) {
			setFilterStatus("Start time must be before end time.", "error")
			return
		}
		filterStart = newStart
		filterEnd = newEnd
		resetSegments()
		await loadMoreSegments(true)
	}

	const clearFilters = async () => {
		filterStart = null
		filterEnd = null
		startInput.input.value = ""
		endInput.input.value = ""
		resetSegments()
		await loadMoreSegments(true)
	}

	applyFiltersButton.onClick = applyFilters
	clearFiltersButton.onClick = clearFilters

	startInput.input.value = formatDateInputValue(filterStart)
	endInput.input.value = formatDateInputValue(filterEnd)
	await loadMoreSegments(true)
}

export const segmentPage = async (root: HTMLElement, segmentId: number) => {
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
}
