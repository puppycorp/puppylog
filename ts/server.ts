import { Container, KeyValueTable } from "./ui"
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

	let info: ServerInfo | null = null
	try {
		info = await fetch("/api/v1/server_info").then((r) => r.json())
	} catch (e) {
		root.add(new KeyValueTable([{ key: "Error", value: String(e) }]))
		return
	}

	if (!info) {
		root.add(new KeyValueTable([{ key: "Error", value: "No data" }]))
		return
	}

	root.add(
		new KeyValueTable([
			{ key: "Total space", value: formatBytes(info.totalBytes) },
			{
				key: "Used space",
				value: `${formatBytes(info.usedBytes)} (${info.usedPercent.toFixed(1)}%)`,
			},
			{ key: "Free space", value: formatBytes(info.freeBytes) },
			{ key: "Upload files", value: info.uploadFilesCount.toString() },
			{ key: "Upload bytes", value: formatBytes(info.uploadBytes) },
		]),
	)
}
