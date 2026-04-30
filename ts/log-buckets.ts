const API_BASE = "/api/v1/buckets"

export const MAX_BUCKET_ENTRIES = 200

export type BucketProp = {
	key: string
	value: string
}

export type BucketLogEntry = {
	id: string
	timestamp: string
	level: "trace" | "debug" | "info" | "warn" | "error" | "fatal"
	msg: string
	props: BucketProp[]
}

export type LogBucket = {
	id: string
	name: string
	query: string
	createdAt: string
	updatedAt: string
	logs: BucketLogEntry[]
}

type BucketResponse = {
	id: number | string
	name: string
	query: string
	createdAt: string
	updatedAt: string
	logs: BucketLogEntryResponse[]
}

type BucketLogEntryResponse = {
	id: string
	timestamp: string
	level: string
	msg: string
	props: BucketPropResponse[]
}

type BucketPropResponse = {
	key: string
	value: string
}

type UpsertRequest = {
	id?: number
	name: string
	query: string
}

type AppendLogsRequest = {
	logs: BucketLogEntry[]
}

const isLogLevel = (value: string): value is BucketLogEntry["level"] =>
	["trace", "debug", "info", "warn", "error", "fatal"].includes(value)

const normalizeProp = (prop: BucketPropResponse): BucketProp | null => {
	if (typeof prop !== "object" || prop === null) return null
	if (typeof prop.key !== "string" || prop.key.trim() === "") return null
	if (typeof prop.value !== "string") return null
	return {
		key: prop.key,
		value: prop.value,
	}
}

const normalizeLogEntry = (
	entry: BucketLogEntryResponse,
): BucketLogEntry | null => {
	if (typeof entry !== "object" || entry === null) return null
	if (typeof entry.id !== "string" || entry.id === "") return null
	if (typeof entry.timestamp !== "string" || entry.timestamp === "")
		return null
	const level =
		typeof entry.level === "string" && isLogLevel(entry.level)
			? entry.level
			: null
	if (!level) return null
	const props = Array.isArray(entry.props)
		? entry.props
				.map(normalizeProp)
				.filter((prop): prop is BucketProp => Boolean(prop))
		: []
	return {
		id: entry.id,
		timestamp: entry.timestamp,
		level,
		msg: typeof entry.msg === "string" ? entry.msg : "",
		props,
	}
}

const normalizeBucket = (bucket: BucketResponse): LogBucket | null => {
	if (typeof bucket !== "object" || bucket === null) return null
	if (typeof bucket.name !== "string" || bucket.name.trim() === "")
		return null
	const id = typeof bucket.id === "number" ? bucket.id.toString() : bucket.id
	if (typeof id !== "string" || id === "") return null
	const query = typeof bucket.query === "string" ? bucket.query : ""
	const createdAt =
		typeof bucket.createdAt === "string"
			? bucket.createdAt
			: new Date().toISOString()
	const updatedAt =
		typeof bucket.updatedAt === "string" ? bucket.updatedAt : createdAt
	const logs = Array.isArray(bucket.logs)
		? bucket.logs
				.map(normalizeLogEntry)
				.filter((entry): entry is BucketLogEntry => Boolean(entry))
		: []
	return {
		id,
		name: bucket.name,
		query,
		createdAt,
		updatedAt,
		logs,
	}
}

const handleResponse = async <T>(response: Response): Promise<T> => {
	if (response.status === 204) {
		return null as T
	}
	if (!response.ok) {
		throw new Error(`Bucket request failed: ${response.status}`)
	}
	return (await response.json()) as T
}

export const listBuckets = async (): Promise<LogBucket[]> => {
	const response = await fetch(API_BASE, {
		headers: { Accept: "application/json" },
	})
	const data = await handleResponse<unknown>(response)
	if (!Array.isArray(data)) return []
	return data
		.map((item) => normalizeBucket(item as BucketResponse))
		.filter((bucket): bucket is LogBucket => Boolean(bucket))
}

const prepareUpsertPayload = (args: {
	id?: string
	name: string
	query: string
}): UpsertRequest => {
	const payload: UpsertRequest = {
		name: args.name,
		query: args.query,
	}
	if (args.id !== undefined) {
		const parsed = Number.parseInt(args.id, 10)
		if (!Number.isNaN(parsed)) payload.id = parsed
	}
	return payload
}

const sendBucketRequest = async (
	url: string,
	init: RequestInit,
): Promise<LogBucket | null> => {
	const response = await fetch(url, {
		headers: {
			"Content-Type": "application/json",
			Accept: "application/json",
		},
		...init,
	})
	if (response.status === 404) return null
	const data = await handleResponse<unknown>(response)
	if (!data) return null
	const bucket = normalizeBucket(data as BucketResponse)
	if (!bucket) throw new Error("Received malformed bucket from server")
	return bucket
}

export const upsertBucket = async (args: {
	id?: string
	name: string
	query: string
}): Promise<LogBucket> => {
	const bucket = await sendBucketRequest(API_BASE, {
		method: "POST",
		body: JSON.stringify(prepareUpsertPayload(args)),
	})
	if (!bucket) throw new Error("Failed to create or update bucket")
	return bucket
}

export const appendLogsToBucket = async (
	bucketId: string,
	logs: BucketLogEntry[],
	_limit = MAX_BUCKET_ENTRIES,
): Promise<LogBucket | null> => {
	const payload: AppendLogsRequest = {
		logs,
	}
	return sendBucketRequest(
		`${API_BASE}/${encodeURIComponent(bucketId)}/logs`,
		{
			method: "POST",
			body: JSON.stringify(payload),
		},
	)
}

export const clearBucketLogs = async (
	bucketId: string,
): Promise<LogBucket | null> =>
	sendBucketRequest(`${API_BASE}/${encodeURIComponent(bucketId)}/clear`, {
		method: "POST",
		body: "",
	})

export const deleteBucket = async (bucketId: string): Promise<boolean> => {
	const response = await fetch(
		`${API_BASE}/${encodeURIComponent(bucketId)}`,
		{
			method: "DELETE",
		},
	)
	if (response.status === 404) return false
	if (!response.ok)
		throw new Error(`Failed to delete bucket: ${response.status}`)
	return true
}
