import { logsSearchPage, FetchMoreArgs, LogEntry } from "./logs"
import { Navbar } from "./navbar"
import { Container } from "./ui"
import { formatBytes, formatNumber } from "./utility"
import type { DeviceSetting } from "./devices"

export const devicePage = async (root: HTMLElement, deviceId: string) => {
	root.innerHTML = ""
	const container = new Container(root)
	const navbar = new Navbar()
	container.add(navbar)

	const res = await fetch("/api/v1/devices")
	const devices = (await res.json()) as DeviceSetting[]
	const device = devices.find((d) => d.id === deviceId)
	if (!device) {
		container.root.textContent = "Device not found"
		return
	}

	const details = document.createElement("div")
	details.className = "device-details"
	const addDetail = (k: string, v: string) => {
		const div = document.createElement("div")
		div.innerHTML = `<strong>${k}:</strong> ${v}`
		details.appendChild(div)
	}
	addDetail("ID", device.id)
	addDetail("Created", new Date(device.createdAt).toLocaleString())
	addDetail("Last upload", new Date(device.lastUploadAt).toLocaleString())
	addDetail("Send logs", device.sendLogs ? "Yes" : "No")
	addDetail("Filter level", device.filterLevel)
	addDetail("Send interval", device.sendInterval.toString())
	addDetail("Logs count", formatNumber(device.logsCount))
	addDetail("Logs size", formatBytes(device.logsSize))
	for (const prop of device.props) addDetail(prop.key, prop.value)
	container.root.appendChild(details)

	const logsContainer = document.createElement("div")
	container.root.appendChild(logsContainer)

	const buildQuery = (q?: string) =>
		q && q.trim()
			? `deviceId=\"${deviceId}\" AND (${q})`
			: `deviceId=\"${deviceId}\"`

	logsSearchPage({
		root: logsContainer,
		streamLogs: (
			args: FetchMoreArgs,
			onNew: (l: LogEntry) => void,
			onEnd: () => void,
		) => {
			const fullQuery = buildQuery(args.query)
			const params = new URLSearchParams()
			if (fullQuery) params.append("query", fullQuery)
			if (args.count) params.append("count", args.count.toString())
			if (args.endDate) params.append("endDate", args.endDate)
			params.append("tzOffset", new Date().getTimezoneOffset().toString())
			const url = new URL("/api/logs", window.location.origin)
			url.search = params.toString()
			const es = new EventSource(url)
			es.onmessage = (ev) => onNew(JSON.parse(ev.data))
			es.onerror = () => {
				es.close()
				onEnd()
			}
			return () => es.close()
		},
		validateQuery: async (query: string) => {
			const q = buildQuery(query)
			const res = await fetch(
				`/api/v1/validate_query?query=${encodeURIComponent(q)}`,
			)
			if (res.status === 200) return null
			return res.text()
		},
	})
}
