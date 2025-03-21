import { showModal } from "./common"
import { Button, Container, HList, Label, Select, SelectGroup, TextInput, UiComponent, vlist, VList } from "./ui"
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

const bulkEdit = async (args: {
	deviceIds: string[]
	sendLogs: boolean
	filterLevel: string
	sendInterval: number
}) => {
	await fetch(`/api/v1/device/bulkedit`, {
		method: "POST",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify(args),
	})
}

type Prop = {
	key: string
	value: string
}

type DeviceSetting = {
	id: string
	sendLogs: boolean
	filterLevel: string
	logsSize: number
	logsCount: number
	sendInterval: number
	createdAt: string
	lastUploadAt: string
	updated?: boolean
	props: Prop[]
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

		const sendIntervalCell = document.createElement("div")
		sendIntervalCell.className = "table-cell"
		sendIntervalCell.innerHTML = `<strong>Send interval:</strong> ${device.sendInterval} seconds`
		this.root.appendChild(sendIntervalCell)

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

		const propsContainer = document.createElement("div")
		propsContainer.className = "table-cell"
		const propsTitle = document.createElement("strong")
		propsTitle.textContent = "Props:"
		propsContainer.appendChild(propsTitle)
		if (device.props.length === 0) {
			const noPropsRow = document.createElement("div")
			noPropsRow.textContent = "No properties"
			propsContainer.appendChild(noPropsRow)
		} else {
			device.props.forEach(prop => {
				const propRow = document.createElement("div")
				propRow.textContent = `${prop.key} = ${prop.value}`
				propsContainer.appendChild(propRow)
			})
		}
		this.root.appendChild(propsContainer)

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
		this.root.style.display = "flex"
		this.root.style.flexDirection = "row"
		this.root.style.flexWrap = "wrap"
		this.root.style.gap = "5px"
		this.root.style.overflowX = "auto"
		this.root.style.padding = "16px"
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
		totalDevicesCount: number
		totalLogsCount: number
		totalLogsSize: number
		averageLogSize: number
		totalLogsPerSecond: number
	}) {
		this.root.innerHTML = `
			<div><strong>Total Devices Count:</strong> ${formatNumber(args.totalDevicesCount)}</div>
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
		label: "Sending logs",
		value: "all",
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
	const bulkEditButton = document.createElement("button")
	bulkEditButton.textContent = "Bulk Edit"

	const searchOptions = new HList()
	searchOptions.root.style.margin = "10px"
	searchOptions.root.style.gap = "10px"
	searchOptions.add(sendLogsSearchOption)
	searchOptions.root.appendChild(bulkEditButton)
	const devicesList = new DevicesList()
	page.add(header, searchOptions, devicesList)

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
			totalDevicesCount: devices.length,
			totalLogsCount,
			totalLogsSize,
			averageLogSize,
			totalLogsPerSecond: totalLogsPerSecond
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
		let filteredDevices = devices

		bulkEditButton.onclick = () => {
			const first = filteredDevices[0]
			if (!first) return
			const bulkEditFilterLevel = new SelectGroup({
				label: "Filter level",
				value: first.filterLevel,
				options: levels.map(level => ({ text: level, value: level }))
			})
			const sendLogsSelect = new SelectGroup({
				label: "Send logs",
				value: first.sendLogs ? "true" : "false",
				options: [
					{ text: "Yes", value: "true" },
					{ text: "No", value: "false" }
				]
			})
			const sendIntervalInput = new TextInput({ 
				label: "Send interval", 
				placeholder: "Enter interval",
				value: first.sendInterval.toString()
			})

			const saveButton = new Button({ text: "Save" })
			saveButton.onClick = async () => {
				const filterLevel = bulkEditFilterLevel.value
				const sendLogs = sendLogsSelect.value === "true"
				await bulkEdit({
					deviceIds: filteredDevices.map(p => p.id),
					filterLevel,
					sendInterval: parseInt(sendIntervalInput.value),
					sendLogs
				})
				for (const device of filteredDevices) {
					device.filterLevel = filterLevel
					device.sendLogs = sendLogs
					device.sendInterval = parseInt(sendIntervalInput.value)
				}
				renderList(filteredDevices)
			}

			showModal({
				title: "Bulk Edit",
				minWidth: 300,
				content: new VList({
					style: {
						gap: "10px"
					}
				}).add(
					bulkEditFilterLevel,
					sendLogsSelect,
					sendIntervalInput,
					new Label({ text: "Devices: " }),
					new Label({ text: filteredDevices.map(p => p.id).join(", ") }),
				).root,
				footer: [saveButton]
			})
		}

		sendLogsSearchOption.onChange = async (value) => {
			filteredDevices = devices.filter(device => {
				return device.sendLogs === (value === "true") || (value === "all")	
			})
			renderList(filteredDevices)
		}
	} catch (error) {
		console.error("Error fetching devices:", error)
		const devicesList = document.getElementById("devicesList")
		if (devicesList) {
			devicesList.innerHTML = `<p>Error fetching devices. Please try again later.</p>`
		}
	}
}