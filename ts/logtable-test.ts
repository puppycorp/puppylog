import { LogEntry, logsSearchPage } from "./logs"

/**
 * Utility function that generates a string of random uppercase letters.
 * Optionally inserts a given number of line breaks at random positions.
 */
function logline(length: number, linebreaks: number): string {
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

/**
 * Utility that randomly generates a log line.
 */
function randomLogline(): string {
	const length = Math.floor(Math.random() * 100)
	const linebreaks = Math.floor(Math.random() * 10)
	return logline(length, linebreaks)
}

/**
 * This function sets up the logs UI in test mode.
 * It provides a simulated `fetchMore` function that returns fake logs
 * and a simulated `streamLogs` that pushes new logs on an interval.
 */
export const logtableTest = (root: HTMLElement): HTMLElement => {
	logsSearchPage({
		root,
		fetchMore: async (args) => {
			// simulate network delay
			await new Promise((resolve) => setTimeout(resolve, 500));
			const logs: Array<LogEntry> = [];
			const count = args.count || 100;
			for (let i = 0; i < count; i++) {
				logs.push({
					id: `${Date.now()}-${i}`,
					timestamp: new Date(Date.now() - i * 1000).toISOString(),
					level: "info",
					props: [
						{ key: "key", value: "value" },
						{ key: "key2", value: "value2" }
					],
					msg: `[${i}] ${randomLogline()}`
				});
			}
			return logs;
		},
		streamLogs: (
			query: string,
			onNewLog: (log: LogEntry) => void,
			onEnd: () => void
		) => {
			const intervalId = setInterval(() => {
				onNewLog({
					id: `${Date.now()}-stream`,
					timestamp: new Date().toISOString(),
					level: "debug",
					props: [{ key: "stream", value: "true" }],
					msg: `Streamed log: ${randomLogline()}`
				});
			}, 2000);
			const timeoutId = setTimeout(() => {
				clearInterval(intervalId);
				onEnd();
			}, 10000);
			return () => {
				clearInterval(intervalId);
				clearTimeout(timeoutId);
			};
		}
	});
	return root;
};

