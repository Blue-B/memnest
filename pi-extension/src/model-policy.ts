// Client-side routing safeguard, not an attestation or a complete DLP boundary.
// A trusted proxy can forward elsewhere; already-sent/history text is not erased.
const allowed = new Set(
  (process.env.MEMNEST_ALLOWED_MODEL_PROVIDERS ?? "")
    .split(",").map(value => value.trim()).filter(Boolean),
);
export function memoryReadAllowed(ctx?: { model?: { provider?: string } }): boolean {
  return allowed.size === 0 || (typeof ctx?.model?.provider === "string" && allowed.has(ctx.model.provider));
}
export const MODEL_POLICY_ERROR = "Memory retrieval blocked by MEMNEST_ALLOWED_MODEL_PROVIDERS. Use an approved model in a fresh session; this safeguard does not erase earlier context.";
