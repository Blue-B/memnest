// Where credentials may travel. Plain HTTP to another machine shows a bearer
// token to the network, so it goes to this machine or over TLS only.
const host = (url: string): { hostname: string; protocol: string } | undefined => {
	try {
		const parsed = new URL(url);
		return { hostname: parsed.hostname, protocol: parsed.protocol };
	} catch {
		return undefined;
	}
};

export function isLoopback(url: string): boolean {
	return ["127.0.0.1", "localhost", "[::1]"].includes(host(url)?.hostname ?? "");
}

export function bearerAllowed(url: string): boolean {
	return isLoopback(url) || host(url)?.protocol === "https:";
}

export const BEARER_REFUSED = "MEMNEST_TOKEN is only sent to a loopback or https MEMNEST_URL";
