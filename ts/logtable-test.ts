import { LogEntry, logsSearchPage } from "./logs"

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

function randomLogline(len: number): string {
	const linebreaks = Math.floor(Math.random() * 10)
	return logline(len, linebreaks)
}

const createRandomJson = (
	totalPropsCount: number,
	maxDepth: number = 5,
): any => {
	const root: Record<string, any> = {}
	let createdCount = 0
	const queue: Array<{ obj: Record<string, any>; depth: number }> = []
	queue.push({ obj: root, depth: 0 })

	while (queue.length > 0 && createdCount < totalPropsCount) {
		const { obj, depth } = queue.shift()!
		const remaining = totalPropsCount - createdCount
		// Limit the number of properties added at this level to a maximum of 10 or the remaining count
		const numProps = Math.floor(Math.random() * Math.min(remaining, 10)) + 1

		for (let i = 0; i < numProps && createdCount < totalPropsCount; i++) {
			const key = `key${createdCount}`
			if (depth < maxDepth && Math.random() > 0.5) {
				const nestedObj: Record<string, any> = {}
				obj[key] = nestedObj
				createdCount++
				queue.push({ obj: nestedObj, depth: depth + 1 })
			} else {
				obj[key] = `value${createdCount}`
				createdCount++
			}
		}
	}

	return root
}

interface XmlNode {
	tag: string
	children?: XmlNode[]
	text?: string
}

const createRandomXml = (
	totalNodesCount: number,
	maxDepth: number = 5,
): string => {
	const root: XmlNode = { tag: "root", children: [] }
	let createdCount = 0
	const queue: Array<{ node: XmlNode; depth: number }> = [
		{ node: root, depth: 0 },
	]
	while (queue.length > 0 && createdCount < totalNodesCount) {
		const { node, depth } = queue.shift()!
		const remaining = totalNodesCount - createdCount
		const numChildren =
			Math.floor(Math.random() * Math.min(remaining, 10)) + 1
		node.children = node.children || []
		for (
			let i = 0;
			i < numChildren && createdCount < totalNodesCount;
			i++
		) {
			const tagName = `element${createdCount}`
			if (depth < maxDepth && Math.random() > 0.5) {
				const childNode: XmlNode = { tag: tagName, children: [] }
				node.children.push(childNode)
				createdCount++
				queue.push({ node: childNode, depth: depth + 1 })
			} else {
				const childNode: XmlNode = {
					tag: tagName,
					text: `value${createdCount}`,
				}
				node.children.push(childNode)
				createdCount++
			}
		}
	}
	const nodeToXml = (node: XmlNode): string => {
		if (node.children && node.children.length > 0) {
			const childrenXml = node.children
				.map((child) => nodeToXml(child))
				.join("")
			return `<${node.tag}>${childrenXml}</${node.tag}>`
		} else if (node.text !== undefined) {
			return `<${node.tag}>${node.text}</${node.tag}>`
		} else {
			return `<${node.tag}/>`
		}
	}
	return nodeToXml(root)
}

export const logtableTest = (root: HTMLElement): HTMLElement => {
	logsSearchPage({
		root,
		streamLogs: (args, onNewLog, onEnd) => {
			onNewLog({
				id: `${Date.now()}-text`,
				timestamp: new Date().toISOString(),
				level: "debug",
				props: [],
				msg: `Streamed log: ${randomLogline(100_000)}`,
			})
			const randomPropsCount = Math.floor(Math.random() * 50) + 1
			const randomPropsObject = createRandomJson(700)
			onNewLog({
				id: `${Date.now()}-json`,
				timestamp: new Date().toISOString(),
				level: "debug",
				props: [],
				msg: `JSON ${JSON.stringify(randomPropsObject)}`,
			})
			const randomXml = createRandomXml(1000)
			onNewLog({
				id: `${Date.now()}-xml`,
				timestamp: new Date().toISOString(),
				level: "debug",
				props: [],
				msg: `XML ${randomXml}`,
			})
			onEnd()
			return () => {}
		},
		validateQuery: async (query) => {
			return null
		},
	})
	return root
}
