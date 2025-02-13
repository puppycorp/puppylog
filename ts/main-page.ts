import { LogEntry, logsSearchPage } from "./logs"
import { Router } from "./router";
import { getQueryParam, removeQueryParam, setQueryParam } from "./utility";

export const mainPage = (root: HTMLElement) => {
	let query: string | undefined = getQueryParam("query") || ""
	let logEventSource: EventSource | null = null  
	let isStreaming = getQueryParam("stream") === "true"
	let lastStreamQuery: string | null = null
	let logEntriesBuffer: LogEntry[] = []
	let timeout: any = null
	const startStream = (query: string | undefined) => {
		if (lastStreamQuery === query) return
		lastStreamQuery = query
		if (logEventSource) logEventSource.close()
		logEventSource = null
		const streamQuery = new URLSearchParams()
		if (query) streamQuery.append("query", query)
		const streamUrl = new URL("/api/logs/stream", window.location.origin)
		streamUrl.search = streamQuery.toString()
		logEventSource = new EventSource(streamUrl)
		logEventSource.onopen = () => setIsStreaming(true)
		logEventSource.onmessage = (event) => {
			const data = JSON.parse(event.data)
			logEntriesBuffer.push(data)
			if (timeout) return
			timeout = setTimeout(() => {
				addLogEntries(logEntriesBuffer)
				logEntriesBuffer = []
				timeout = null
			}, 30);
		}
		logEventSource.onerror = (event) => {
			console.error("EventSource error", event)
			if (logEventSource) logEventSource.close()
			setIsStreaming(false)
		}
	}
	const { addLogEntries, onError, setIsStreaming } = logsSearchPage({
		root,
		isStreaming,
		query,
		toggleIsStreaming: () => {
			isStreaming = !isStreaming
			if (isStreaming) {
				startStream(query)
				setQueryParam("stream", "true")
			} else {
				if (logEventSource) logEventSource.close()
				lastStreamQuery = null
				removeQueryParam("stream")
			}
			return isStreaming
		},
		fetchMore: (args) => {
			query = args.query
			if (query) setQueryParam("query", query)
			else removeQueryParam("query")
			console.log("fetchMore", args)
			const urlQuery = new URLSearchParams()
			const offsetInMinutes = new Date().getTimezoneOffset();
			const offsetInHours = -offsetInMinutes / 60;
			urlQuery.append("timezone", offsetInHours.toString())
			if (args.query) urlQuery.append("query", args.query)
			if (args.count) urlQuery.append("count", args.count.toString())
			if (args.endDate) urlQuery.append("endDate", args.endDate)
			const url = new URL("/api/logs", window.location.origin)
			url.search = urlQuery.toString()
			fetch(url.toString()).then(async (res) => {
				if (res.status === 400) {
					const err = await res.json()
					console.error("res.error", err)
					onError(err.error)
					console.log("res", res)
					throw new Error("Failed to fetch logs")
				}
				return res.json()
			}).then((data) => {
				addLogEntries(data)
			}).catch((err) => {
				console.error("error", err)
			})
			if (isStreaming) startStream(query)
			else {
				if (logEventSource) {
					logEventSource.close()
				}
			}
		}
	})
	return root
}