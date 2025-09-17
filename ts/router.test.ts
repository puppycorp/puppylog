import { expect, test } from "bun:test"

if (typeof window === "undefined") {
	;(globalThis as any).window = { location: { pathname: "/" } }
}

test("asdf", () => {
	expect(window.location.pathname).toBe("/")
})
