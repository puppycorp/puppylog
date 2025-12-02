import {
	Container,
	VList,
	Button,
	SelectGroup,
	TextInput,
	KeyValueTable,
} from "./ui"
import { Navbar } from "./navbar"
import { formatBytes, formatNumber } from "./utility"
import { DeviceSetting, levels } from "./devices"
import type { Prop } from "./types"

type DeviceSegment = {
	id: number
	firstTimestamp: string
	lastTimestamp: string
	originalSize: number
	compressedSize: number
	logsCount: number
}

const SEGMENTS_PAGE_SIZE = 20

const fetchDevice = async (deviceId: string): Promise<DeviceSetting> => {
	const response = await fetch(
		`/api/v1/device/${encodeURIComponent(deviceId)}`,
	)
	if (response.status === 404) {
		throw new Error("Device not found")
	}
	if (!response.ok) {
		throw new Error(await response.text())
	}
	return (await response.json()) as DeviceSetting
}

const fetchDeviceSegments = async (
	deviceId: string,
	options?: { count?: number; end?: Date | null },
): Promise<DeviceSegment[]> => {
	const url = new URL("/api/segments", window.location.origin)
	url.searchParams.append("device_ids[]", deviceId)
	const count = options?.count ?? SEGMENTS_PAGE_SIZE
	url.searchParams.set("count", count.toString())
	url.searchParams.set("sort", "desc")
	if (options?.end) url.searchParams.set("end", options.end.toISOString())
	const response = await fetch(url.toString())
	if (!response.ok) throw new Error(await response.text())
	return (await response.json()) as DeviceSegment[]
}

const downloadSegmentLogs = async (
	segmentId: number,
	button: Button,
	statusEl: HTMLElement,
) => {
	const previousText = button.root.textContent || "Download logs"
	setStatus(statusEl, "", "idle")
	button.root.disabled = true
	button.root.textContent = "Downloading..."
	try {
		const res = await fetch(`/api/v1/segment/${segmentId}/logs.txt`)
		if (!res.ok) throw new Error(await res.text())
		const blob = await res.blob()
		const url = URL.createObjectURL(blob)
		const anchor = document.createElement("a")
		anchor.href = url
		anchor.download = `segment-${segmentId}.txt`
		document.body.appendChild(anchor)
		anchor.click()
		anchor.remove()
		URL.revokeObjectURL(url)
	} catch (error) {
		setStatus(
			statusEl,
			error instanceof Error
				? error.message || "Failed to download segment logs."
				: "Failed to download segment logs.",
			"error",
		)
	} finally {
		button.root.disabled = false
		button.root.textContent = previousText
	}
}

const createSegmentCard = (segment: DeviceSegment, statusEl: HTMLElement) => {
	const table = new KeyValueTable([
		{
			key: "Segment",
			value: `#${segment.id}`,
			href: `/segment/${segment.id}`,
		},
		{
			key: "First timestamp",
			value: formatDate(segment.firstTimestamp),
		},
		{
			key: "Last timestamp",
			value: formatDate(segment.lastTimestamp),
		},
		{
			key: "Logs count",
			value: formatNumber(segment.logsCount),
		},
		{
			key: "Original size",
			value: formatBytes(segment.originalSize),
		},
		{
			key: "Compressed size",
			value: formatBytes(segment.compressedSize),
		},
	])
	const segmentCard = new VList({
		style: { gap: "8px" },
	})
	segmentCard.root.classList.add("summary")
	segmentCard.add(table)
	const downloadButton = new Button({
		text: "Download logs (.txt)",
	})
	downloadButton.onClick = () =>
		downloadSegmentLogs(segment.id, downloadButton, statusEl)
	segmentCard.add(downloadButton)
	return segmentCard
}

const updateDeviceSettings = async (
	deviceId: string,
	payload: { sendLogs: boolean; filterLevel: string; sendInterval: number },
) => {
	const response = await fetch(
		`/api/v1/device/${encodeURIComponent(deviceId)}/settings`,
		{
			method: "POST",
			headers: { "Content-Type": "application/json" },
			body: JSON.stringify(payload),
		},
	)
	if (!response.ok) {
		throw new Error(await response.text())
	}
}

const updateDeviceMetadata = async (deviceId: string, props: Prop[]) => {
	const response = await fetch(
		`/api/v1/device/${encodeURIComponent(deviceId)}/metadata`,
		{
			method: "POST",
			headers: { "Content-Type": "application/json" },
			body: JSON.stringify(props),
		},
	)
	if (!response.ok) {
		throw new Error(await response.text())
	}
}

const createSection = (title: string) => {
	const section = new VList({
		style: {
			gap: "12px",
		},
	})
	section.root.classList.add("summary")
	const heading = document.createElement("h2")
	heading.textContent = title
	heading.style.margin = "0"
	section.root.appendChild(heading)
	return section
}

const formatDate = (value: string | null) => {
	if (!value) return "Never"
	const parsed = new Date(value)
	return Number.isNaN(parsed.getTime()) ? "Never" : parsed.toLocaleString()
}

const logsPerSecond = (device: DeviceSetting) => {
	if (!device.createdAt || !device.lastUploadAt) return 0
	const createdAt = new Date(device.createdAt)
	const lastUpload = new Date(device.lastUploadAt)
	const diffSeconds = (lastUpload.getTime() - createdAt.getTime()) / 1000
	if (!Number.isFinite(diffSeconds) || diffSeconds <= 0) return 0
	return device.logsCount / diffSeconds
}

const setStatus = (
	element: HTMLElement,
	message: string,
	type: "idle" | "info" | "error",
) => {
	element.textContent = message
	switch (type) {
		case "info":
			element.style.color = "#047857"
			break
		case "error":
			element.style.color = "#b91c1c"
			break
		default:
			element.style.color = ""
	}
}

export const devicePage = async (root: HTMLElement, deviceId: string) => {
	root.innerHTML = ""
	const page = new Container(root)
	const navbar = new Navbar()
	page.add(navbar)

	const content = new VList({
		style: {
			gap: "16px",
		},
	})
	content.root.style.padding = "16px"
	content.root.style.maxWidth = "960px"
	content.root.style.margin = "0 auto"
	page.add(content)

	const title = document.createElement("h1")
	title.textContent = `Device ${deviceId}`
	title.style.margin = "0"
	content.root.appendChild(title)

	const loading = document.createElement("div")
	loading.className = "logs-loading-indicator"
	loading.textContent = "Loading device..."
	content.root.appendChild(loading)

	let device: DeviceSetting
	try {
		device = await fetchDevice(deviceId)
	} catch (error) {
		loading.textContent =
			error instanceof Error
				? error.message || "Failed to load device"
				: "Failed to load device"
		loading.classList.remove("logs-loading-indicator")
		loading.style.color = "#b91c1c"
		return
	}

	content.root.removeChild(loading)
	title.textContent = `Device ${device.id}`

	const stats = new KeyValueTable([
		{ key: "Created", value: formatDate(device.createdAt) },
		{ key: "Last upload", value: formatDate(device.lastUploadAt) },
		{ key: "Logs count", value: formatNumber(device.logsCount) },
		{ key: "Logs size", value: formatBytes(device.logsSize) },
		{
			key: "Average log size",
			value:
				device.logsCount === 0
					? "0 Bytes"
					: formatBytes(device.logsSize / device.logsCount),
		},
		{
			key: "Logs per second",
			value: logsPerSecond(device).toFixed(2),
		},
	])
	content.add(stats)

	const settingsSection = createSection("Settings")
	content.add(settingsSection)

	const filterLevelSelect = new SelectGroup({
		label: "Filter level",
		value: device.filterLevel,
		options: levels.map((level) => ({
			text: level,
			value: level,
		})),
	})

	const sendLogsSelect = new SelectGroup({
		label: "Send logs",
		value: device.sendLogs ? "true" : "false",
		options: [
			{ text: "Yes", value: "true" },
			{ text: "No", value: "false" },
		],
	})

	const sendIntervalInput = new TextInput({
		label: "Send interval (seconds)",
		value: device.sendInterval.toString(),
	})
	const sendIntervalInputEl = sendIntervalInput.root.querySelector(
		"input",
	) as HTMLInputElement | null
	if (sendIntervalInputEl) {
		sendIntervalInputEl.type = "number"
		sendIntervalInputEl.min = "0"
	}

	const saveSettingsButton = new Button({ text: "Save settings" })
	saveSettingsButton.root.disabled = true

	const settingsStatus = document.createElement("div")
	setStatus(settingsStatus, "", "idle")

	let settingsDirty = false
	const markSettingsDirty = () => {
		if (!settingsDirty) {
			saveSettingsButton.root.disabled = false
			settingsDirty = true
		}
		setStatus(settingsStatus, "", "idle")
	}

	filterLevelSelect.onChange = () => markSettingsDirty()
	sendLogsSelect.onChange = () => markSettingsDirty()
	if (sendIntervalInputEl) {
		sendIntervalInputEl.oninput = () => markSettingsDirty()
	}

	saveSettingsButton.onClick = async () => {
		if (!settingsDirty) return
		const interval = sendIntervalInputEl
			? parseInt(sendIntervalInputEl.value, 10)
			: device.sendInterval
		if (!Number.isFinite(interval) || interval < 0) {
			setStatus(
				settingsStatus,
				"Send interval must be a non-negative number",
				"error",
			)
			return
		}
		saveSettingsButton.root.disabled = true
		setStatus(settingsStatus, "Saving settings...", "idle")
		try {
			await updateDeviceSettings(device.id, {
				sendLogs: sendLogsSelect.value === "true",
				filterLevel: filterLevelSelect.value,
				sendInterval: interval,
			})
			device.sendLogs = sendLogsSelect.value === "true"
			device.filterLevel = filterLevelSelect.value
			device.sendInterval = interval
			settingsDirty = false
			setStatus(settingsStatus, "Settings saved", "info")
		} catch (error) {
			setStatus(
				settingsStatus,
				error instanceof Error
					? error.message || "Failed to save settings"
					: "Failed to save settings",
				"error",
			)
			saveSettingsButton.root.disabled = false
		}
	}

	settingsSection.add(
		filterLevelSelect,
		sendLogsSelect,
		sendIntervalInput,
		saveSettingsButton,
	)
	settingsSection.root.appendChild(settingsStatus)

	const metadataSection = createSection("Metadata")
	content.add(metadataSection)

	let props: Prop[] = device.props
		? device.props.map((prop) => ({ ...prop }))
		: []
	const propsList = new VList({
		style: {
			gap: "8px",
		},
	})

	const metadataStatus = document.createElement("div")
	setStatus(metadataStatus, "", "idle")

	const metadataSaveButton = new Button({ text: "Save metadata" })
	metadataSaveButton.root.disabled = true

	let metadataDirty = false
	const markMetadataDirty = () => {
		metadataDirty = true
		metadataSaveButton.root.disabled = false
		setStatus(metadataStatus, "", "idle")
	}

	const renderProps = () => {
		propsList.root.innerHTML = ""
		if (props.length === 0) {
			const empty = document.createElement("div")
			empty.textContent = "No metadata"
			empty.style.color = "#6b7280"
			propsList.root.appendChild(empty)
			return
		}
		props.forEach((prop, index) => {
			const row = document.createElement("div")
			row.style.display = "flex"
			row.style.flexWrap = "wrap"
			row.style.gap = "8px"
			row.style.alignItems = "center"

			const keyInput = document.createElement("input")
			keyInput.type = "text"
			keyInput.placeholder = "Key"
			keyInput.value = prop.key
			keyInput.oninput = () => {
				props[index].key = keyInput.value
				markMetadataDirty()
			}

			const valueInput = document.createElement("input")
			valueInput.type = "text"
			valueInput.placeholder = "Value"
			valueInput.value = prop.value
			valueInput.oninput = () => {
				props[index].value = valueInput.value
				markMetadataDirty()
			}

			const removeButton = document.createElement("button")
			removeButton.textContent = "Remove"
			removeButton.onclick = () => {
				props.splice(index, 1)
				renderProps()
				markMetadataDirty()
			}

			row.append(keyInput, valueInput, removeButton)
			propsList.root.appendChild(row)
		})
	}

	renderProps()

	const addPropButton = new Button({ text: "Add property" })
	addPropButton.onClick = () => {
		props.push({ key: "", value: "" })
		renderProps()
		markMetadataDirty()
	}

	metadataSaveButton.onClick = async () => {
		if (!metadataDirty) return
		const sanitized = props
			.map((prop) => ({ key: prop.key.trim(), value: prop.value.trim() }))
			.filter((prop) => prop.key.length > 0)
		metadataSaveButton.root.disabled = true
		setStatus(metadataStatus, "Saving metadata...", "idle")
		try {
			await updateDeviceMetadata(device.id, sanitized)
			device.props = sanitized
			props = sanitized.map((prop) => ({ ...prop }))
			renderProps()
			metadataDirty = false
			setStatus(metadataStatus, "Metadata saved", "info")
		} catch (error) {
			setStatus(
				metadataStatus,
				error instanceof Error
					? error.message || "Failed to save metadata"
					: "Failed to save metadata",
				"error",
			)
			metadataSaveButton.root.disabled = false
		}
	}

	metadataSection.add(propsList, addPropButton, metadataSaveButton)
	metadataSection.root.appendChild(metadataStatus)

	const segmentsSection = createSection("Segments")
	const segmentsList = new VList({
		style: {
			gap: "12px",
		},
	})
	const segmentsStatus = document.createElement("div")
	setStatus(segmentsStatus, "Loading segments…", "idle")
	segmentsSection.add(segmentsList)
	const loadMoreButton = new Button({ text: "Load more segments" })
	loadMoreButton.root.style.alignSelf = "flex-start"
	loadMoreButton.root.style.display = "none"
	segmentsSection.add(loadMoreButton)
	segmentsSection.root.appendChild(segmentsStatus)
	content.add(segmentsSection)

	let segmentsEndDate: Date | null = null
	let segmentsExhausted = false
	let segmentsLoading = false

	const renderSegments = (segments: DeviceSegment[]) => {
		segments.forEach((segment) => {
			const card = createSegmentCard(segment, segmentsStatus)
			segmentsList.add(card)
		})
	}

	const updateLoadMoreVisibility = (lastBatchSize: number) => {
		if (segmentsExhausted || lastBatchSize < SEGMENTS_PAGE_SIZE) {
			loadMoreButton.root.style.display = "none"
		} else {
			loadMoreButton.root.style.display = "inline-flex"
			loadMoreButton.root.disabled = false
			loadMoreButton.root.textContent = "Load more segments"
		}
	}

	const loadSegments = async (append: boolean) => {
		if (segmentsLoading) return
		segmentsLoading = true
		if (!append) {
			segmentsList.root.innerHTML = ""
			setStatus(segmentsStatus, "Loading segments…", "idle")
		} else {
			loadMoreButton.root.disabled = true
			loadMoreButton.root.textContent = "Loading…"
		}
		try {
			const segments = await fetchDeviceSegments(device.id, {
				count: SEGMENTS_PAGE_SIZE,
				end: segmentsEndDate,
			})
			if (segments.length === 0 && !append) {
				setStatus(
					segmentsStatus,
					"No segments for this device yet.",
					"idle",
				)
				loadMoreButton.root.style.display = "none"
				segmentsExhausted = true
			} else {
				setStatus(segmentsStatus, "", "idle")
				renderSegments(segments)
				if (segments.length > 0) {
					const last = segments[segments.length - 1]
					segmentsEndDate = new Date(last.lastTimestamp)
				}
				if (segments.length < SEGMENTS_PAGE_SIZE) {
					segmentsExhausted = true
				}
				updateLoadMoreVisibility(segments.length)
			}
		} catch (error) {
			setStatus(
				segmentsStatus,
				error instanceof Error
					? error.message || "Failed to load segments."
					: "Failed to load segments.",
				"error",
			)
			loadMoreButton.root.style.display = "inline-flex"
			loadMoreButton.root.disabled = false
			loadMoreButton.root.textContent = "Retry"
		} finally {
			segmentsLoading = false
		}
	}

	loadMoreButton.onClick = () => loadSegments(true)
	await loadSegments(false)
}
