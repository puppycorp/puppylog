import { devicesPage } from "./devices"
import { logtableTest } from "./logtable-test"
import { mainPage } from "./main-page"
import { PivotPage } from "./pivot"
import { routes } from "./router"
import { segmentPage, segmentsPage } from "./segment-page"
import { settingsPage } from "./settings"
import { Container } from "./ui"
import { Navbar } from "./navbar"
import { queriesPage } from "./queries"

window.onload = () => {
	const body = document.querySelector("body")
	if (!body) {
		throw new Error("No body element found")
	}
	const container = new Container(body)
	const navbar = new Navbar()
	container.add(navbar)
	const pageRoot = document.createElement("div")
	const pageContainer = new Container(pageRoot)
	container.add(pageContainer)

	const renderElem =
		(handler: (root: HTMLElement, nav: Navbar) => any) => () => {
			pageRoot.innerHTML = ""
			navbar.setRight()
			handler(pageRoot, navbar)
		}
	const renderContainer =
		(handler: (root: Container, nav: Navbar) => any) => () => {
			pageRoot.innerHTML = ""
			navbar.setRight()
			handler(pageContainer, navbar)
		}
	routes({
		"/tests/logs": renderElem(logtableTest),
		"/settings": renderContainer(settingsPage),
		"/devices": renderElem(devicesPage),
		"/segments": renderContainer(segmentsPage),
		"/queries": renderElem(queriesPage),
		"/segment/:segmentId": (params: any) => {
			pageRoot.innerHTML = ""
			navbar.setRight()
			segmentPage(pageRoot, params.segmentId, navbar)
		},
		"/pivot": renderElem(PivotPage),
		"/*": renderElem(mainPage),
	})
}
