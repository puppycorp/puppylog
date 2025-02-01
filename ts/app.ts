import { routes } from "./router";
import { logtableTest } from "./logtable-test";
import { mainPage } from "./main-page";
import { settingsPage } from "./settings";

window.onload = () => {
	const body = document.querySelector("body");

	if (!body) {
		throw new Error("No body element found")
	}

	const navigate = routes({
		"/tests": () => {
			const tests = document.createElement("div")
			tests.innerHTML = "Tests"
			return tests
		},
		"/tests/logtable": () => logtableTest(),
		"/settings": () => settingsPage(),
		"/*": () => mainPage()
	}, body)
}