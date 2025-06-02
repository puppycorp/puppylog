import { Header } from "./ui";
import { navigate } from "./router";

export type SavedQuery = {
        name: string
        query: string
}

export const loadSavedQueries = (): SavedQuery[] => {
        try {
                const raw = localStorage.getItem("savedQueries")
                if (!raw) return []
                return JSON.parse(raw) as SavedQuery[]
        } catch {
                return []
        }
}

export const saveQuery = (item: SavedQuery) => {
        const items = loadSavedQueries()
        items.push(item)
        localStorage.setItem("savedQueries", JSON.stringify(items))
}

export const queriesPage = (root: HTMLElement) => {
        root.innerHTML = ""
        const header = new Header({ title: "Saved Queries" })
        root.appendChild(header.root)
        const list = document.createElement("div")
        list.style.display = "flex"
        list.style.flexDirection = "column"
        list.style.gap = "10px"
        list.style.padding = "16px"
        root.appendChild(list)
        const items = loadSavedQueries()
        items.forEach(item => {
                const row = document.createElement("div")
                row.className = "list-row"
                row.textContent = item.name
                row.onclick = () => navigate(`/?query=${encodeURIComponent(item.query)}`)
                list.appendChild(row)
        })
        return root
}
