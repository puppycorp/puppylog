import {
	createAuthControls,
	getAuthConfig,
	getCurrentUser,
	onAuthStateChange,
	renderGoogleSignInButton,
	AuthenticatedUser,
	AuthConfig,
} from "./auth"
import { Navbar } from "./navbar"

const formatAllowedDomains = (config: AuthConfig): string | null => {
	const domains = config.google?.allowedDomains
	if (!domains || domains.length === 0) return null
	return domains.join(", ")
}

const updateAccountBanner = (
	banner: HTMLDivElement,
	avatar: HTMLImageElement,
	nameEl: HTMLSpanElement,
	emailEl: HTMLSpanElement,
	summary: HTMLParagraphElement,
	user: AuthenticatedUser | null,
) => {
	if (user) {
		banner.style.display = "flex"
		if (user.picture) {
			avatar.src = user.picture
			avatar.alt = user.name || user.email || "Account"
			avatar.style.display = "block"
		} else {
			avatar.removeAttribute("src")
			avatar.alt = "Account"
			avatar.style.display = "none"
		}
		const displayName = user.name || user.email || "Signed in"
		nameEl.textContent = displayName
		if (user.email && user.email !== displayName) {
			emailEl.textContent = user.email
			emailEl.style.display = "block"
		} else {
			emailEl.textContent = ""
			emailEl.style.display = "none"
		}
		summary.textContent = "You're signed in. You can switch accounts below."
	} else {
		banner.style.display = "none"
		summary.textContent = "Choose a sign-in provider to continue."
	}
}

export const signInPage = async (root: HTMLElement) => {
	root.innerHTML = ""
	const navbar = new Navbar({ right: [createAuthControls()] })
	root.appendChild(navbar.root)

	const page = document.createElement("div")
	page.className = "signin-page"
	root.appendChild(page)

	const title = document.createElement("h1")
	title.textContent = "Sign in"
	page.appendChild(title)

	const summary = document.createElement("p")
	summary.className = "signin-summary"
	summary.textContent = "Choose a sign-in provider to continue."
	page.appendChild(summary)

	const accountBanner = document.createElement("div")
	accountBanner.className = "signin-account"
	accountBanner.style.display = "none"
	page.appendChild(accountBanner)

	const accountAvatar = document.createElement("img")
	accountAvatar.className = "signin-account__avatar"
	accountAvatar.alt = "Account"
	accountAvatar.style.display = "none"
	accountAvatar.referrerPolicy = "no-referrer"
	accountBanner.appendChild(accountAvatar)

	const accountInfo = document.createElement("div")
	accountInfo.className = "signin-account__info"
	accountBanner.appendChild(accountInfo)

	const accountLabel = document.createElement("span")
	accountLabel.className = "signin-account__label"
	accountLabel.textContent = "Signed in as"
	accountInfo.appendChild(accountLabel)

	const accountName = document.createElement("span")
	accountName.className = "signin-account__name"
	accountInfo.appendChild(accountName)

	const accountEmail = document.createElement("span")
	accountEmail.className = "signin-account__email"
	accountInfo.appendChild(accountEmail)

	const providers = document.createElement("div")
	providers.className = "signin-providers"
	page.appendChild(providers)

	const config = await getAuthConfig()

	if (!config.enabled || !config.google?.clientId) {
		summary.textContent =
			"Authentication is not configured for this server."
		providers.style.display = "none"
		return
	}

	const googleCard = document.createElement("div")
	googleCard.className = "signin-provider"
	providers.appendChild(googleCard)

	const googleTitle = document.createElement("h2")
	googleTitle.className = "signin-provider__title"
	googleTitle.textContent = "Google"
	googleCard.appendChild(googleTitle)

	const googleDetails = document.createElement("p")
	googleDetails.className = "signin-provider__details"
	googleDetails.textContent = "Use your Google account to access PuppyLog."
	googleCard.appendChild(googleDetails)

	const allowedDomains = formatAllowedDomains(config)
	if (allowedDomains) {
		const domains = document.createElement("p")
		domains.className = "signin-provider__domains"
		domains.textContent = `Allowed domains: ${allowedDomains}`
		googleCard.appendChild(domains)
	}

	const googleButton = document.createElement("div")
	googleButton.className = "signin-provider__button"
	googleCard.appendChild(googleButton)

	try {
		const rendered = await renderGoogleSignInButton(googleButton, config, {
			theme: "outline",
			size: "large",
		})
		if (!rendered) {
			googleButton.textContent =
				"Google sign-in is currently unavailable."
		}
	} catch (error) {
		console.error("Failed to render Google sign-in button", error)
		googleButton.textContent = "Google sign-in is currently unavailable."
	}

	const initialUser = getCurrentUser()
	updateAccountBanner(
		accountBanner,
		accountAvatar,
		accountName,
		accountEmail,
		summary,
		initialUser,
	)
	onAuthStateChange((user) =>
		updateAccountBanner(
			accountBanner,
			accountAvatar,
			accountName,
			accountEmail,
			summary,
			user,
		),
	)
}
