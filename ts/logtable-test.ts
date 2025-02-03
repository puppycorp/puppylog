import { LogEntry, logsSearchPage } from "./logs"

const logline = (length: number, linebreaks: number) => {
	let line = ""
	for (let i = 0; i < length; i++) {
		line += String.fromCharCode(65 + Math.floor(Math.random() * 26))
	}
	for (let i = 0; i < linebreaks; i++) {
		const idx = Math.floor(Math.random() * (line.length + 1))
		line = line.slice(0, idx) + "\n" + line.slice(idx)
	}
	return line
}

const randomLogline = () => {
	const length = Math.floor(Math.random() * 100)
	const linebreaks = Math.floor(Math.random() * 10)
	return logline(length, linebreaks)
}

export const logtableTest = (root: HTMLElement) => {
	const { addLogEntries } = logsSearchPage({
		root,
		isStreaming: false,
		toggleIsStreaming: () => false,
		fetchMore: (args) => {
			const logEntries: LogEntry[] = []
			for (let i = args.offset; i < args.offset + args.count; i++) {
				logEntries.push({
					id: i.toString(),
					timestamp: new Date().toISOString(),
					level: "info",
					props: [
						{ key: "key", value: "value" },
						{ key: "key2", value: "value2" }
					],
					msg: `[${i}] ${randomLogline()}`
				})
			}
			addLogEntries(logEntries)
		}
	}) 

	return root
}