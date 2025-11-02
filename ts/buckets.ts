import { showModal } from "./common"
import { formatLogMsg } from "./logmsg"
import { navigate } from "./router"
import { Navbar } from "./navbar"
import {
	clearBucketLogs,
	deleteBucket,
	listBuckets,
	MAX_BUCKET_ENTRIES,
} from "./log-buckets"
import type { LogBucket } from "./log-buckets"

const formatTimestamp = (value: string): string => {
	const date = new Date(value)
	if (Number.isNaN(date.getTime())) return "unknown time"
	const yyyy = date.getFullYear()
	const mm = String(date.getMonth() + 1).padStart(2, "0")
	const dd = String(date.getDate()).padStart(2, "0")
	const hh = String(date.getHours()).padStart(2, "0")
	const mi = String(date.getMinutes()).padStart(2, "0")
	const ss = String(date.getSeconds()).padStart(2, "0")
	return `${yyyy}-${mm}-${dd} ${hh}:${mi}:${ss}`
}

const truncate = (value: string, length: number): string =>
	value.length > length ? `${value.slice(0, length)}…` : value

const emptyMessage = () => {
	const empty = document.createElement("div")
	empty.textContent =
		"No buckets yet. Collect logs from the search page to populate this view."
	empty.style.color = "#6b7280"
	empty.style.fontStyle = "italic"
	return empty
}

const buildActions = (bucket: LogBucket, refresh: () => Promise<void>) => {
	const actions = document.createElement("div")
	actions.style.display = "flex"
	actions.style.gap = "8px"
	actions.style.flexWrap = "wrap"

	const openButton = document.createElement("button")
	openButton.textContent = "Open in search"
	openButton.onclick = () =>
		navigate(`/?query=${encodeURIComponent(bucket.query)}`)
	actions.appendChild(openButton)

	const clearButton = document.createElement("button")
	clearButton.textContent = "Clear logs"
	clearButton.onclick = () => {
		void (async () => {
			try {
				await clearBucketLogs(bucket.id)
				await refresh()
			} catch (error) {
				console.error("Failed to clear bucket", error)
				window.alert("Failed to clear bucket logs. Please try again.")
			}
		})()
	}
	actions.appendChild(clearButton)

	const deleteButton = document.createElement("button")
	deleteButton.textContent = "Delete bucket"
	deleteButton.onclick = () => {
		if (
			window.confirm(
				`Delete bucket "${bucket.name}"? This only removes the copies.`,
			)
		) {
			void (async () => {
				try {
					await deleteBucket(bucket.id)
					await refresh()
				} catch (error) {
					console.error("Failed to delete bucket", error)
					window.alert("Failed to delete bucket. Please try again.")
				}
			})()
		}
	}
	actions.appendChild(deleteButton)

	return actions
}

const buildLogList = (bucket: LogBucket) => {
	const list = document.createElement("ul")
	list.style.display = "flex"
	list.style.flexDirection = "column"
	list.style.gap = "6px"

	bucket.logs.forEach((entry) => {
		const item = document.createElement("li")
		item.style.listStyle = "none"
		item.style.padding = "8px"
		item.style.border = "1px solid #e5e7eb"
		item.style.borderRadius = "4px"
		item.style.cursor = "pointer"

		const header = document.createElement("div")
		header.textContent = `${formatTimestamp(entry.timestamp)} · ${entry.level.toUpperCase()}`
		header.style.fontSize = "12px"
		header.style.color = "#374151"
		item.appendChild(header)

		if (entry.props.length > 0) {
			const propsLine = document.createElement("div")
			propsLine.textContent = entry.props
				.map((prop) => `${prop.key}=${prop.value}`)
				.join(" ")
			propsLine.style.fontSize = "12px"
			propsLine.style.color = "#6b7280"
			item.appendChild(propsLine)
		}

		const message = document.createElement("div")
		message.textContent = truncate(entry.msg, 160)
		message.style.whiteSpace = "pre-wrap"
		item.appendChild(message)

		item.onclick = () =>
			showModal({
				title: "Log Message",
				content: formatLogMsg(entry.msg),
				footer: [],
			})
		list.appendChild(item)
	})

	return list
}

export const bucketsPage = (root: HTMLElement) => {
	root.innerHTML = ""
	const navbar = new Navbar()
	root.appendChild(navbar.root)
	const container = document.createElement("div")
	container.style.display = "flex"
	container.style.flexDirection = "column"
	container.style.gap = "12px"
	container.style.padding = "16px"
	root.appendChild(container)

	const render = async () => {
		container.innerHTML = ""
		const loading = document.createElement("div")
		loading.textContent = "Loading buckets…"
		loading.style.color = "#6b7280"
		container.appendChild(loading)

		let buckets: LogBucket[] = []
		try {
			buckets = await listBuckets()
		} catch (error) {
			console.error("Failed to load buckets", error)
			container.innerHTML = ""
			const errorNotice = document.createElement("div")
			errorNotice.textContent =
				"Failed to load buckets. Please try again later."
			errorNotice.style.color = "#dc2626"
			container.appendChild(errorNotice)
			return
		}

		container.innerHTML = ""
		if (buckets.length === 0) {
			container.appendChild(emptyMessage())
			return
		}

		buckets.forEach((bucket) => {
			const section = document.createElement("section")
			section.style.display = "flex"
			section.style.flexDirection = "column"
			section.style.gap = "8px"
			section.style.border = "1px solid #e5e7eb"
			section.style.borderRadius = "6px"
			section.style.padding = "12px"
			container.appendChild(section)

			const title = document.createElement("div")
			title.textContent = `${bucket.name} · ${bucket.logs.length}/${MAX_BUCKET_ENTRIES} logs`
			title.style.fontWeight = "600"
			section.appendChild(title)

			const query = document.createElement("div")
			query.textContent = bucket.query
				? `Query: ${bucket.query}`
				: "Query: (none)"
			query.style.fontSize = "12px"
			query.style.color = "#6b7280"
			section.appendChild(query)

			section.appendChild(buildActions(bucket, render))

			if (bucket.logs.length === 0) {
				const empty = document.createElement("div")
				empty.textContent = "Bucket is empty."
				empty.style.color = "#6b7280"
				section.appendChild(empty)
				return
			}

			section.appendChild(buildLogList(bucket))
		})
	}

	void render()

	return root
}
