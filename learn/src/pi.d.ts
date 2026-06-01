// Minimal ambient declarations for the pi runtime packages the integration
// layer (index.ts) targets. These mirror the surface used by the shipping
// extensions jayzeng/pi-memory and chandra447/pi-hermes-memory, and let this
// package typecheck standalone. When the real packages are installed they
// supersede these shims.

declare module "@earendil-works/pi-coding-agent" {
  export interface PiModel {
    provider: string;
    id: string;
  }
  export interface ExtensionContext {
    model?: PiModel;
    hasUI: boolean;
    isIdle(): boolean;
    sessionManager: { getSessionId(): string };
    ui: { notify(message: string, level?: string): void };
  }
  export interface BeforeAgentStartEvent {
    prompt?: string;
    systemPrompt: string;
  }
  export interface InputEvent {
    source?: string;
    text: string;
  }
  export interface ToolResult {
    content: Array<{ type: "text"; text: string }>;
    details?: Record<string, unknown>;
  }
  export interface ToolDef {
    name: string;
    description: string;
    parameters: unknown;
    execute(
      toolCallId: string,
      params: any,
      signal: AbortSignal | undefined,
      onUpdate: unknown,
      ctx: ExtensionContext,
    ): Promise<ToolResult>;
  }
  export interface ExtensionAPI {
    on(event: string, handler: (event: any, ctx: ExtensionContext) => unknown): void;
    registerTool(tool: ToolDef): void;
  }
}

declare module "@earendil-works/pi-ai" {
  import type { PiModel } from "@earendil-works/pi-coding-agent";
  export interface CompleteResponse {
    content: Array<{ type: string; text?: string }>;
  }
  export function complete(
    model: PiModel,
    request: { systemPrompt: string; messages: Array<{ role: string; content: Array<{ type: string; text: string }>; timestamp?: number }> },
    options?: { apiKey?: string; reasoningEffort?: "low" | "medium" | "high" },
  ): Promise<CompleteResponse>;
}
