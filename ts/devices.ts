import { Container, Select, SelectGroup, UiComponent } from "./ui"
import { formatBytes, formatNumber } from "./utility"

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

export class DeviceRow extends UiComponent<HTMLDivElement> {
	device: DeviceSetting

	constructor(device: DeviceSetting) {
		const deviceRow = document.createElement("div")
		deviceRow.classList.add("list-row")
		super(deviceRow)
		this.device = device

		// ID cell
		const idCell = document.createElement("div")
		idCell.className = "table-cell"
		idCell.innerHTML = `<strong>ID:</strong> ${device.id}`
		this.root.appendChild(idCell)

		// Created at cell
		const createdAtCell = document.createElement("div")
		createdAtCell.className = "table-cell"
		createdAtCell.innerHTML = `<strong>Created at:</strong> ${new Date(device.createdAt).toLocaleString()}`
		this.root.appendChild(createdAtCell)

		// Filter level cell with select
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
		this.root.appendChild(filterLevelCell)

		// Last upload cell
		const lastUploadCell = document.createElement("div")
		lastUploadCell.className = "table-cell"
		lastUploadCell.innerHTML = `<strong>Last upload:</strong> ${new Date(device.lastUploadAt).toLocaleString()}`
		this.root.appendChild(lastUploadCell)

		// Logs count cell
		const logsCountCell = document.createElement("div")
		logsCountCell.className = "table-cell"
		logsCountCell.innerHTML = `<strong>Logs count:</strong> ${formatNumber(device.logsCount)}`
		this.root.appendChild(logsCountCell)

		// Logs size cell
		const logsSizeCell = document.createElement("div")
		logsSizeCell.className = "table-cell"
		logsSizeCell.innerHTML = `<strong>Logs size:</strong> ${formatBytes(device.logsSize)} bytes`
		this.root.appendChild(logsSizeCell)

		// Average log size cell
		const averageLogSizeCell = document.createElement("div")
		averageLogSizeCell.className = "table-cell"
		averageLogSizeCell.innerHTML = `<strong>Average log size:</strong> ${formatBytes(device.logsSize / device.logsCount)}`
		this.root.appendChild(averageLogSizeCell)

		// Logs per second cell
		const logsPerSecondCell = document.createElement("div")
		logsPerSecondCell.className = "table-cell"
		const lastUploadDate = new Date(device.lastUploadAt)
		const createdAtDate = new Date(device.createdAt)
		const diff = lastUploadDate.getTime() - createdAtDate.getTime()
		const seconds = diff / 1000
		const logsPerSecond = device.logsCount / seconds
		logsPerSecondCell.innerHTML = `<strong>Logs per second:</strong> ${logsPerSecond.toFixed(2)}`
		this.root.appendChild(logsPerSecondCell)

		// Send logs cell
		const sendLogsCell = document.createElement("div")
		sendLogsCell.className = "table-cell"
		sendLogsCell.innerHTML = `<strong>Send logs:</strong> ${device.sendLogs ? "Yes" : "No"}`
		this.root.appendChild(sendLogsCell)

		// Save button
		const deviceSaveButton = document.createElement("button")
		deviceSaveButton.textContent = "Save"
		deviceSaveButton.style.visibility = "hidden"
		this.root.appendChild(deviceSaveButton)

		const markDirty = () => {
			deviceSaveButton.style.visibility = "visible"
		}

		// Event listeners
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
	}
}

class DevicesList implements UiComponent<HTMLDivElement> {
	public readonly root: HTMLDivElement

	constructor() {
		this.root = document.createElement("div")
		this.root.classList.add("logs-list")
		this.root.innerHTML = `<div class="logs-loading-indicator">Loading devices...</div>`
	}

	public add(device: DeviceRow) {
		this.root.appendChild(device.root)
	}

	public noDevicesFound() {
		this.root.innerHTML = `<p>No devices found.</p>`
	}

	public clear() {
		this.root.innerHTML = ""
	}
}

class Summary extends UiComponent<HTMLDivElement> {
	constructor() {
		super(document.createElement("div"))
		this.root.innerHTML = ""
	}

	public setSummary(args: {
		totalLogsCount: number
		totalLogsSize: number
		averageLogSize: number
		totalLogsPerSecond: number
	}) {
		this.root.innerHTML = `
			<div><strong>Total Logs Count:</strong> ${formatNumber(args.totalLogsCount)}</div>
			<div><strong>Total Logs Size:</strong> ${formatBytes(args.totalLogsSize)}</div>
			<div><strong>Average Log Size:</strong> ${formatBytes(args.averageLogSize)}</div>
			<div><strong>Logs per Second:</strong> ${args.totalLogsPerSecond.toFixed(2)}</div>
		`
	}
}

class Header extends UiComponent<HTMLDivElement> {
	constructor(args: {
		title: string
		rightSide?: UiComponent<HTMLElement>
	}) {
		super(document.createElement("div"))
		this.root.className = "page-header"
		const title = document.createElement("h1")
		title.textContent = args.title
		title.style.flexGrow = "1"
		this.root.appendChild(title)
		if (args.rightSide) {
			this.root.append(args.rightSide.root)
		}
	}
}

class SearchOptions extends UiComponent<HTMLDivElement> {
	constructor() {
		super(document.createElement("div"))
		this.root.style.display = "flex"
		this.root.style.flexDirection = "row"
	}
}

export const devicesPage = async (root: HTMLElement) => {
	const page = new Container(root)
	const summary = new Summary()
	const header = new Header({
		title: "Devices",
		rightSide: summary
	})
	const sendLogsSearchOption = new SelectGroup({
		label: "Send logs",
		options: [
			{
				text: "All",
				value: "all"
			},
			{
				text: "Yes",
				value: "true"
			},
			{
				text: "No",
				value: "false"
			}
		]
	})
	// const content = new Container(document.createElement("div"))
	const devicesList = new DevicesList()
	page.add(header, sendLogsSearchOption, devicesList)

	try {
		const res = await fetch("/api/v1/devices")
		const devices = await res.json() as DeviceSetting[]
		let totalLogsCount = 0, totalLogsSize = 0
		let earliestTimestamp = Infinity, latestTimestamp = -Infinity
		let totalLogsPerSecond = 0
		devices.forEach(device => {
			totalLogsCount += device.logsCount
			totalLogsSize += device.logsSize
			const createdAtTime = new Date(device.createdAt).getTime()
			const lastUploadTime = new Date(device.lastUploadAt).getTime()
			earliestTimestamp = Math.min(earliestTimestamp, createdAtTime)
			latestTimestamp = Math.max(latestTimestamp, lastUploadTime)
			const logsPersecond = device.logsCount / ((lastUploadTime - createdAtTime) / 1000)
			if (!isNaN(logsPersecond)) totalLogsPerSecond += logsPersecond
		})
		const totalSeconds = (latestTimestamp - earliestTimestamp) / 1000
		const averageLogSize = totalLogsCount > 0 ? totalLogsSize / totalLogsCount : 0
		summary.setSummary({
			totalLogsCount,
			totalLogsSize,
			averageLogSize,
			totalLogsPerSecond: totalLogsPerSecond / devices.length
		})

		const renderList = (devices: DeviceSetting[]) => {
			devicesList.clear()
			if (Array.isArray(devices) && devices.length > 0) {
				for (const device of devices) {
					devicesList.add(new DeviceRow(device))
				}
			} else {
				devicesList.noDevicesFound()
			}
		}

		renderList(devices)
		sendLogsSearchOption.onChange = async (value) => {
			const filteredDevices = devices.filter(device => {
				return device.sendLogs === (value === "true") || (value === "all")	
			})
			renderList(filteredDevices)
		}

		// const devicesList = document.getElementById("devicesList")
		// if (!devicesList) return
		// devicesList.innerHTML = ""
		
		// 	devices.forEach(device => {
		// 		devicesList.appendChild(new DeviceRow(device).root)
		// 	})
		// } else {
			
		// }
	} catch (error) {
		console.error("Error fetching devices:", error)
		const devicesList = document.getElementById("devicesList")
		if (devicesList) {
			devicesList.innerHTML = `<p>Error fetching devices. Please try again later.</p>`
		}
	}
}

// root.innerHTML = `
// <div class="page-header">
// 	<h1 style="flex-grow: 1">Devices</h1>
// 	<div id="devicesSummary">Loading summary...</div>
// </div>
// <div>
// 	<div style="display: flex; flex-direction: row"> 
// 		<div style="display: flex; flex-direction: column; margin-right: 1rem">
// 			<div>
// 				Search options
// 			</div>
// 			<div>
// 				Send logs
// 			</div>
// 			<select>
// 				<option>Yes</option>
// 				<option>No</option>
// 			</select>
// 		</div>
// 		<div style="display: flex; flex-direction: column; margin-right: 1rem">
// 			<div>
// 				Filter level:
// 				<select>
// 					<option>trace</option>
// 					<option>debug</option>
// 					<option>info</option>
// 					<option>warn</option>
// 					<option>error</option>
// 					<option>fatal</option>
// 				</select>
// 			</div>
// 			<div>
// 				Send logs:
// 				<select>
// 					<option>Yes</option>
// 					<option>No</option>
// 				</select>
// 			</div>
// 			<div>
// 				Send Interval:
// 				<input type="text" />
// 			</div>
// 			<button>Save</button>
// 		</div>
// 	</div>

// </div>
// <div id="devicesList">
// 	<div class="logs-loading-indicator">Loading devices...</div>
// </div>

// `