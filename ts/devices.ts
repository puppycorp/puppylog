const saveDeviceSettings = async (device: DeviceSetting) => {
	await fetch(`/api/v1/device/${device.id}/settings`, {
		method: "POST",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify({
			sendLogs: device.sendLogs,
			filterLevel: device.filterLevel,
		}),
	})
}
const formatBytes = (bytes: number, decimals = 2): string => {
	if (bytes === 0) return "0 Bytes"
	const k = 1024
	const dm = decimals < 0 ? 0 : decimals
	const sizes = ["Bytes", "KB", "MB", "GB", "TB", "PB", "EB", "ZB", "YB"]
	const i = Math.floor(Math.log(bytes) / Math.log(k))
	return parseFloat((bytes / Math.pow(k, i)).toFixed(dm)) + " " + sizes[i]
}
type DeviceSetting = {
	id: string
	sendLogs: boolean
	filterLevel: string
	logsSize: number
	logsCount: number
	createdAt: string
	lastUploadAt: string
	updated?: boolean
}
const levels = ["trace", "debug", "info", "warn", "error", "fatal"]
const createDeviceRow = (device: DeviceSetting): HTMLElement => {
	const deviceRow = document.createElement("div")
	deviceRow.classList.add("list-row")

	const idCell = document.createElement("div")
	idCell.className = "table-cell"
	idCell.innerHTML = `<strong>ID:</strong> ${device.id}`
	deviceRow.appendChild(idCell)

	const createdAtCell = document.createElement("div")
	createdAtCell.className = "table-cell"
	createdAtCell.innerHTML = `<strong>Created at:</strong> ${new Date(device.createdAt).toLocaleString()}`
	deviceRow.appendChild(createdAtCell)

	const filterLevelCell = document.createElement("div")
	filterLevelCell.className = "table-cell"
	filterLevelCell.innerHTML = `<strong>Filter level:</strong> `
	const select = document.createElement("select")
	levels.forEach(level => {
		const option = document.createElement("option")
		option.value = level
		option.textContent = level
		select.appendChild(option)
	})
	select.value = device.filterLevel
	filterLevelCell.appendChild(select)
	deviceRow.appendChild(filterLevelCell)

	const lastUploadCell = document.createElement("div")
	lastUploadCell.className = "table-cell"
	lastUploadCell.innerHTML = `<strong>Last upload:</strong> ${new Date(device.lastUploadAt).toLocaleString()}`
	deviceRow.appendChild(lastUploadCell)

	const logsCountCell = document.createElement("div")
	logsCountCell.className = "table-cell"
	logsCountCell.innerHTML = `<strong>Logs count:</strong> ${device.logsCount}`
	deviceRow.appendChild(logsCountCell)

	const logsSizeCell = document.createElement("div")
	logsSizeCell.className = "table-cell"
	logsSizeCell.innerHTML = `<strong>Logs size:</strong> ${formatBytes(device.logsSize)} bytes`
	deviceRow.appendChild(logsSizeCell)

	const averageLogSizeCell = document.createElement("div")
	averageLogSizeCell.className = "table-cell"
	averageLogSizeCell.innerHTML = `<strong>Average log size:</strong> ${formatBytes(device.logsSize / device.logsCount)}`
	deviceRow.appendChild(averageLogSizeCell)

	const sendLogsCell = document.createElement("div")
	sendLogsCell.className = "table-cell"
	sendLogsCell.innerHTML = `<strong>Send logs:</strong> ${device.sendLogs ? "Yes" : "No"}`
	deviceRow.appendChild(sendLogsCell)

	const deviceSaveButton = document.createElement("button")
	deviceSaveButton.textContent = "Save"
	deviceSaveButton.style.visibility = "hidden"
	deviceRow.appendChild(deviceSaveButton)

	const markDirty = () => {
		deviceSaveButton.style.visibility = "visible"
	}

	select.onchange = () => {
		device.filterLevel = select.value
		markDirty()
	}

	sendLogsCell.onclick = () => {
		device.sendLogs = !device.sendLogs
		sendLogsCell.innerHTML = `<strong>Send logs:</strong> ${device.sendLogs ? "Yes" : "No"}`
		markDirty()
	}

	deviceSaveButton.onclick = async () => {
		await saveDeviceSettings(device)
		deviceSaveButton.style.visibility = "hidden"
	}

	return deviceRow
}

export const devicesPage = async (root: HTMLElement) => {
	root.innerHTML = `
		<div class="page-header">
			<h1>Devices</h1>
		</div>
		<div id="devicesList">
			<div class="logs-loading-indicator">Loading devices...</div>
		</div>
	`
	try {
		const res = await fetch("/api/v1/devices")
		const devices = await res.json() as DeviceSetting[]
		const devicesList = document.getElementById("devicesList")
		if (!devicesList) return
		devicesList.innerHTML = ""
		if (Array.isArray(devices) && devices.length > 0) {
			devices.forEach(device => {
				devicesList.appendChild(createDeviceRow(device))
			})
		} else {
			devicesList.innerHTML = `<p>No devices found.</p>`
		}
	} catch (error) {
		console.error("Error fetching devices:", error)
		const devicesList = document.getElementById("devicesList")
		if (devicesList) {
			devicesList.innerHTML = `<p>Error fetching devices. Please try again later.</p>`
		}
	}
}