import index from "./assets/index.html"

const API_BASE_URL = process.env.API_BASE_URL
if (!API_BASE_URL) {
	throw new Error("Missing API_BASE_URL in environment")
}

Bun.serve({
	port: 3338,
	routes: {
		"/api/*": async (req) => {
			const reqUrl = new URL(req.url)
			console.log("url", reqUrl.href)
			const targetUrl = new URL(
				reqUrl.pathname + reqUrl.search,
				API_BASE_URL,
			)
			console.log("targetUrl", targetUrl.href)
			const upstreamResponse = await fetch(targetUrl.toString(), {
				method: req.method,
				headers: req.headers,
				body: ["GET", "HEAD"].includes(req.method)
					? undefined
					: req.body,
				redirect: "manual",
			})
			const { status, statusText, headers } = upstreamResponse
			const responseHeaders = new Headers(headers)
			// Remove hop-by-hop headers to prevent connection issues
			responseHeaders.delete("connection")
			responseHeaders.delete("keep-alive")
			responseHeaders.delete("transfer-encoding")
			responseHeaders.delete("upgrade")
			return new Response(upstreamResponse.body, {
				status,
				statusText,
				headers: responseHeaders,
			})
		},
		"/*": index,
	},
})
