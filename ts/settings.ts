
export const settingsPage = (root: HTMLElement) => {
	const infoText = document.createElement("div")
	infoText.style.color = "red"

	let originalQuery = ""
	const updateQuery = (query: string) => {
		fetch("/api/settings/query", { 
			method: "POST",
			body: query 
		}).then(res => {
			if (!res.ok) {
				console.error("Failed to fetch query", res)
				return
			}
			originalQuery = query
			infoText.innerHTML = ""
		}).catch(err => {
			console.error("Failed to update query", err)
		})
	}

	root.innerHTML = ""
	const header = document.createElement("h1")
	header.innerHTML = "Settings"
	root.appendChild(header)

	const collectionQuery = document.createElement("h2")
	collectionQuery.innerHTML = "Collection query"
	root.appendChild(collectionQuery)

	const textarea = document.createElement("textarea")
	textarea.style.width = "100%"
	textarea.style.height = "100px"
	textarea.style.resize = "none"
	root.appendChild(textarea)

	textarea.oninput = (e) => {
		console.log("onchange", textarea.value)
		if (originalQuery === textarea.value) infoText.innerHTML = ""
		else infoText.innerHTML = "Unsaved changes"
	}

	fetch("/api/settings/query").then(res => {
		if (!res.ok) {
			console.error("Failed to fetch query", res)
		}
		return res.text()
	}).then(query => {
		console.log("query", query)
		originalQuery = query
		textarea.value = query
	}).catch(err => {
		console.error("Failed to fetch query", err)
	})

	const saveButton = document.createElement("button")
	saveButton.innerHTML = "Save"
	saveButton.onclick = () => {
		updateQuery(textarea.value)
	}

	// const root = document.createElement("div")	
	root.appendChild(infoText)
	root.appendChild(saveButton)
	// root.innerHTML = `
	// 	<div>
	// 		<h1>Settings</h1>
	// 		<h2>Collection query</h2>
	// 		<div style="width: 100%; height: 100px;">
	// 			<textarea style="width: 100%; height: 100px; resize: none;"></textarea>
	// 		</div>
	// 		<button>Save</button>
	// 	</div>
	// `
	return root
}