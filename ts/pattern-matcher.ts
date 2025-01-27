// type ExtractRouteParams<T extends string> = T extends `${infer Start}*${infer Rest}`
// 	? Record<string, string>
// 	: T extends `${infer Start}:${infer Param}/${infer Rest}`
// 	? { [K in Param]: string } & ExtractRouteParams<Rest>
// 	: T extends `${infer Start}:${infer Param}`
// 	? { [K in Param]: string }
// 	: Record<string, string>;

type ExtractRouteParams<T extends string> = T extends `${infer Start}:${infer Param}/${infer Rest}`
	? { [K in Param]: string } & ExtractRouteParams<Rest>
	: T extends `${infer Start}:${infer Param}`
	? { [K in Param]: string }
	: Record<string, string>

type RouteHandler<Pattern extends string, Result> = (
	params: ExtractRouteParams<Pattern>
) => Result;

type PatternMatcherHandlers = {
	[Pattern in string]: RouteHandler<Pattern, any>;
};

type InferHandlerReturn<H> = H extends RouteHandler<any, infer R> ? R : never;

type MatchResult<T extends PatternMatcherHandlers> = {
	pattern: keyof T;
	result: InferHandlerReturn<T[keyof T]>;
} | null;

function patternMatcher<T extends Record<string, (params: any) => any>>(
    handlers: T
) {
    type TypedHandlers = {
        [K in keyof T]: (params: ExtractRouteParams<K & string>) => ReturnType<T[K]>;
    };
    const typedHandlers = handlers as TypedHandlers;

    const routes = Object.keys(typedHandlers).sort((a, b) => {
        if (!a.includes('*') && !a.includes(':')) return -1;
        if (!b.includes('*') && !b.includes(':')) return 1;
        if (a.includes(':') && !b.includes(':')) return -1;
        if (!a.includes(':') && b.includes(':')) return 1;
        if (a.includes('*') && !b.includes('*')) return 1;
        if (!a.includes('*') && b.includes('*')) return -1;
        return b.length - a.length;
    });

    return {
        match(path: string): { pattern: keyof T; result: ReturnType<T[keyof T]> } | null {
            for (const route of routes) {
                const params = matchRoute(route, path);
                if (params !== null) {
                    const result = typedHandlers[route](params);
                    return { pattern: route, result };
                }
            }
            return null;
        }
    };
}

function matchRoute(pattern: string, path: string): Record<string, string> | null {
	const patternParts = pattern.split('/');
	const pathParts = path.split('/');

	if (pattern === '/*') return {};

	if (patternParts.length !== pathParts.length) {
		const lastPattern = patternParts[patternParts.length - 1];
		if (lastPattern === '*' && pathParts.length >= patternParts.length - 1) {
			return {};
		}
		return null;
	}

	const params: Record<string, string> = {};

	for (let i = 0; i < patternParts.length; i++) {
		const patternPart = patternParts[i];
		const pathPart = pathParts[i];

		if (patternPart === '*') return params;

		if (patternPart.startsWith(':')) {
			const paramName = patternPart.slice(1);
			params[paramName] = pathPart;
			continue;
		}

		if (patternPart !== pathPart) return null;
	}

	return params;
}

export { patternMatcher };