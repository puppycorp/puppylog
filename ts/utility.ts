export const setQueryParam = (field: string, value: string) => {
	const url = new URL(window.location.href)
	url.searchParams.set(field, value)
	window.history.pushState({}, "", url.toString())
}
export const getQueryParam = (field: string): string | null => {
	const url = new URL(window.location.href)
	return url.searchParams.get(field)
}
export const removeQueryParam = (field: string) => {
	const url = new URL(window.location.href)
	url.searchParams.delete(field)
	window.history.pushState({}, "", url.toString())
}
export const formatBytes = (bytes: number, decimals = 2): string => {
	if (bytes === 0) return "0 Bytes"
	const k = 1024
	const dm = decimals < 0 ? 0 : decimals
	const sizes = ["Bytes", "KB", "MB", "GB", "TB", "PB", "EB", "ZB", "YB"]
	const i = Math.floor(Math.log(bytes) / Math.log(k))
	return parseFloat((bytes / Math.pow(k, i)).toFixed(dm)) + " " + sizes[i]
}
export const formatNumber = (num: number, decimals: number = 2): string => {
	if (num === 0) return "0"
	const k = 1000
	const dm = decimals < 0 ? 0 : decimals
	const sizes = ["", "K", "M", "B", "T"]
	const i = Math.floor(Math.log(Math.abs(num)) / Math.log(k))
	return parseFloat((num / Math.pow(k, i)).toFixed(dm)) + sizes[i]
}
export const formatTimestamp = (timestamp: string | number): string => {
	const d = new Date(timestamp)
	return d.toLocaleString()
}
