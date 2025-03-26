import { devicesPage } from "./devices";
import { logtableTest } from "./logtable-test";
import { mainPage } from "./main-page";
import { PivotPage } from "./pivot";
import { routes } from "./router";
import { segmentPage, segmentsPage } from "./segment-page";
import { settingsPage } from "./settings";
import { Container } from "./ui";

window.onload = () => {
	const body = document.querySelector("body");
	if (!body) {
		throw new Error("No body element found")
	}
	routes({
		"/tests/logs": () => logtableTest(body),
		"/settings": () => settingsPage(new Container(body)),
		"/devices": () => devicesPage(body),
		"/segments": () => segmentsPage(body),
		"/segment/:segmentId": (params: any) => segmentPage(body, params.segmentId),
		"/pivot": () => PivotPage(body),
		"/*": () => mainPage(body)
	})
}