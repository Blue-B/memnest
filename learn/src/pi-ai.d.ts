declare module "@earendil-works/pi-ai" {
  export type PiTextContent = { type: "text"; text: string };
  export type PiContent = PiTextContent | { type: string; [key: string]: unknown };

  export function complete(
    model: unknown,
    request: {
      systemPrompt?: string;
      messages: Array<{
        role: string;
        content: PiContent[];
        timestamp?: number;
      }>;
    },
    options?: Record<string, unknown>,
  ): Promise<{ content: PiContent[] }>;
}
