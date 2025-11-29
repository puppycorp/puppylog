import { navigate } from "./router"
import { UiComponent } from "./ui"

export type GoogleConfig = {
	clientId: string
	allowedDomains?: string[]
}

export type AuthConfig = {
	enabled: boolean
	google?: GoogleConfig | null
}

export type AuthenticatedUser = {
	token: string
	email?: string
	name?: string
	picture?: string
	expiresAt?: number
}

type AuthListener = (user: AuthenticatedUser | null) => void

const STORAGE_KEY = "puppylog.googleIdToken"

let configPromise: Promise<AuthConfig> | null = null
let currentUser: AuthenticatedUser | null = null
let listeners: AuthListener[] = []
let googleScriptPromise: Promise<void> | null = null
let googleClientInitializedFor: string | null = null

const notify = () => {
	listeners.forEach((listener) => listener(currentUser))
}

const decodeJwt = (token: string): Record<string, any> | null => {
	const parts = token.split(".")
	if (parts.length < 2) return null
	const base = parts[1].replace(/-/g, "+").replace(/_/g, "/")
	const padded = base + "=".repeat((4 - (base.length % 4)) % 4)
	try {
		const json = atob(padded)
		return JSON.parse(json)
	} catch (err) {
		console.error("Failed to decode token", err)
		return null
	}
}

const storeUserFromToken = (token: string | null) => {
	if (typeof window === "undefined") return
	if (!token) {
		currentUser = null
		window.localStorage.removeItem(STORAGE_KEY)
		notify()
		return
	}
	const payload = decodeJwt(token)
	if (!payload) {
		currentUser = null
		window.localStorage.removeItem(STORAGE_KEY)
		notify()
		return
	}
	const expiresAt =
		typeof payload.exp === "number" ? payload.exp * 1000 : undefined
	if (expiresAt && expiresAt <= Date.now()) {
		currentUser = null
		window.localStorage.removeItem(STORAGE_KEY)
		notify()
		return
	}
	currentUser = {
		token,
		email: payload.email,
		name: payload.name,
		picture: payload.picture,
		expiresAt,
	}
	window.localStorage.setItem(STORAGE_KEY, token)
	notify()
}

const restoreFromStorage = () => {
	if (typeof window === "undefined") return
	const stored = window.localStorage.getItem(STORAGE_KEY)
	if (stored) {
		storeUserFromToken(stored)
	}
}

restoreFromStorage()

const ensureGoogleScript = async (): Promise<void> => {
	if (typeof window === "undefined") return
	if (window.google && window.google.accounts && window.google.accounts.id) {
		return
	}
	if (!googleScriptPromise) {
		googleScriptPromise = new Promise((resolve, reject) => {
			const script = document.createElement("script")
			script.src = "https://accounts.google.com/gsi/client"
			script.async = true
			script.defer = true
			script.onload = () => resolve()
			script.onerror = (event) => reject(event)
			document.head.appendChild(script)
		})
	}
	await googleScriptPromise
}

const initializeGoogleClient = async (clientId: string): Promise<boolean> => {
	await ensureGoogleScript()
	if (!window.google?.accounts?.id) {
		console.warn("Google Identity Services are unavailable")
		return false
	}
	if (googleClientInitializedFor !== clientId) {
		window.google.accounts.id.initialize({
			client_id: clientId,
			callback: handleCredential,
		})
		googleClientInitializedFor = clientId
	}
	return true
}

const hasAuthProviders = (config: AuthConfig): boolean => {
	return Boolean(config.google?.clientId)
}

declare global {
	interface Window {
		google?: any
	}
}

export const getAuthConfig = async (): Promise<AuthConfig> => {
	if (!configPromise) {
		configPromise = fetch("/api/auth/config")
			.then((response) => {
				if (!response.ok) {
					throw new Error(
						`Failed to load auth config: ${response.status}`,
					)
				}
				return response.json() as Promise<AuthConfig>
			})
			.catch((error) => {
				console.warn("Could not load auth config", error)
				return { enabled: false, google: null }
			})
	}
	return configPromise
}

export const getCurrentUser = (): AuthenticatedUser | null => {
	if (currentUser?.expiresAt && currentUser.expiresAt <= Date.now()) {
		storeUserFromToken(null)
	}
	return currentUser
}

export const getIdToken = (): string | null => {
	return getCurrentUser()?.token ?? null
}

export const onAuthStateChange = (listener: AuthListener): (() => void) => {
	listeners.push(listener)
	listener(currentUser)
	return () => {
		listeners = listeners.filter((item) => item !== listener)
	}
}

const handleCredential = (response: { credential: string }) => {
	storeUserFromToken(response.credential)
}

export const signOut = () => {
	if (typeof window !== "undefined" && window.google?.accounts?.id) {
		try {
			window.google.accounts.id.disableAutoSelect()
		} catch (err) {
			console.warn("Failed to disable auto select", err)
		}
	}
	storeUserFromToken(null)
}

export const renderGoogleSignInButton = async (
	container: HTMLElement,
	config?: AuthConfig,
	options?: Record<string, any>,
): Promise<boolean> => {
	if (typeof window === "undefined") return false
	const effectiveConfig = config ?? (await getAuthConfig())
	if (!effectiveConfig.enabled || !effectiveConfig.google?.clientId) {
		return false
	}
	const initialized = await initializeGoogleClient(
		effectiveConfig.google.clientId,
	)
	if (!initialized) {
		return false
	}
	container.innerHTML = ""
	window.google.accounts.id.renderButton(container, {
		theme: "outline",
		size: "large",
		...options,
	})
	return true
}

class AuthControls extends UiComponent<HTMLDivElement> {
	private buttonContainer: HTMLDivElement
	private userContainer: HTMLDivElement
	private unsubscribe?: () => void
	private config: AuthConfig | null = null

	constructor() {
		super(document.createElement("div"))
		this.root.classList.add("auth-controls")
		this.buttonContainer = document.createElement("div")
		this.buttonContainer.classList.add("auth-controls__buttons")
		this.userContainer = document.createElement("div")
		this.userContainer.classList.add("auth-controls__user")
		this.userContainer.style.display = "none"
		this.root.append(this.buttonContainer, this.userContainer)
		this.initialize()
	}

	private async initialize() {
		const config = await getAuthConfig()
		this.config = config
		if (!config.enabled || !hasAuthProviders(config)) {
			this.root.style.display = "none"
			return
		}

		this.unsubscribe = onAuthStateChange((user) => this.render(user))
		this.render(getCurrentUser())
	}

	private render(user: AuthenticatedUser | null) {
		if (user) {
			this.buttonContainer.style.display = "none"
			this.userContainer.style.display = "flex"
			this.userContainer.innerHTML = ""

			if (user.picture) {
				const img = document.createElement("img")
				img.src = user.picture
				img.alt = user.name || user.email || "Account"
				img.width = 32
				img.height = 32
				img.referrerPolicy = "no-referrer"
				img.classList.add("auth-controls__avatar")
				this.userContainer.appendChild(img)
			}

			const label = document.createElement("span")
			label.textContent = user.name || user.email || "Signed in"
			label.classList.add("auth-controls__label")
			this.userContainer.appendChild(label)

			const button = document.createElement("button")
			button.type = "button"
			button.textContent = "Sign out"
			button.classList.add("auth-controls__signout")
			button.onclick = () => signOut()
			this.userContainer.appendChild(button)
		} else {
			if (!this.config || !hasAuthProviders(this.config)) {
				this.root.style.display = "none"
				return
			}
			this.root.style.display = "flex"
			this.buttonContainer.style.display = "flex"
			this.buttonContainer.innerHTML = ""
			this.userContainer.style.display = "none"
			this.userContainer.innerHTML = ""

			const button = document.createElement("button")
			button.type = "button"
			button.textContent = "Sign in"
			button.classList.add("auth-controls__signin")
			button.onclick = (event) => {
				event.preventDefault()
				navigate("/signin")
			}
			this.buttonContainer.appendChild(button)
		}
	}

	public dispose() {
		this.unsubscribe?.()
	}
}

export const createAuthControls = (): UiComponent<HTMLElement> =>
	new AuthControls()
