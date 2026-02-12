import { Button, Container, KeyValueTable } from "./ui"
import { formatBytes } from "./utility"

type ServerInfo = {
	freeBytes: number
	totalBytes: number
	usedBytes: number
	usedPercent: number
	uploadFilesCount: number
	uploadBytes: number
}

export const serverPage = async (root: Container) => {
	root.root.innerHTML = ""

	const loadServerInfo = async (): Promise<ServerInfo | null> => {
		try {
			return await fetch("/api/v1/server_info").then((r) => r.json())
		} catch (e) {
			root.add(new KeyValueTable([{ key: "Error", value: String(e) }]))
			return null
		}
	}

	let info = await loadServerInfo()
	if (!info) {
		root.add(new KeyValueTable([{ key: "Error", value: "No data" }]))
		return
	}

	const infoTable = new KeyValueTable([
		{ key: "Total space", value: formatBytes(info.totalBytes) },
		{
			key: "Used space",
			value: `${formatBytes(info.usedBytes)} (${info.usedPercent.toFixed(1)}%)`,
		},
		{ key: "Free space", value: formatBytes(info.freeBytes) },
		{ key: "Upload files", value: info.uploadFilesCount.toString() },
		{ key: "Upload bytes", value: formatBytes(info.uploadBytes) },
	])
	root.add(infoTable)

	const status = document.createElement("div")
	status.style.padding = "8px 0"
	status.style.fontSize = "12px"
	status.style.color = "#6b7280"
	status.textContent = ""
	root.root.appendChild(status)

	const cleanupButton = new Button({ text: "Start Cleanup" })
	cleanupButton.root.style.width = "fit-content"
	cleanupButton.onClick = async () => {
		cleanupButton.root.disabled = true
		status.style.color = "#6b7280"
		status.textContent = "Starting cleanup..."
		try {
			const res = await fetch("/api/v1/server/cleanup", { method: "POST" })
			if (!res.ok) {
				throw new Error(`cleanup failed (${res.status})`)
			}
			const payload = await res.json()
			const deleted = Number(payload.deletedSegments || 0)
			status.style.color = "#047857"
			status.textContent = `Cleanup finished, deleted ${deleted} segments.`
		} catch (e) {
			status.style.color = "#b91c1c"
			status.textContent =
				e instanceof Error ? e.message : "Failed to start cleanup."
		} finally {
			cleanupButton.root.disabled = false
		}
	}
	root.add(cleanupButton)
}
