import { devicesPage } from "./devices";
import { logtableTest } from "./logtable-test";
import { mainPage } from "./main-page";
import { PivotPage } from "./pivot";
import { routes } from "./router";
import { segmentsPage } from "./segment-page";
import { settingsPage } from "./settings";

window.onload = () => {
	const body = document.querySelector("body");
	if (!body) {
		throw new Error("No body element found")
	}
	routes({
		"/tests/logs": () => logtableTest(body),
		"/settings": () => settingsPage(body),
		"/devices": () => devicesPage(body),
		"/segments": () => segmentsPage(body),
		"/pivot": () => PivotPage(body),
		"/*": () => mainPage(body)
	})
}