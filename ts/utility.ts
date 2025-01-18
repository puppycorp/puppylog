
export const setQueryParam = (field: string, value: string) => {
    const url = new URL(window.location.href)
    url.searchParams.set(field, value)
    window.history.pushState({}, '', url.toString())
}
export const getQueryParam = (field: string): string | null => {
    const url = new URL(window.location.href)
    return url.searchParams.get(field)
}