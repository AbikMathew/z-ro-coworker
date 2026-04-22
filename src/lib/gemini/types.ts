/**
 * Minimal TypeScript types for the Gemini Live BidiGenerateContent protocol.
 *
 * We only type the fields we actually read or write. Everything else is
 * intentionally left loose so upstream schema changes don't break the build.
 *
 * Reference: https://ai.google.dev/api/live
 */

export interface SetupMessage {
  setup: {
    model: string;
    generationConfig?: {
      /** Live API only supports ["AUDIO"]. TEXT output comes via outputAudioTranscription. */
      responseModalities?: ("AUDIO" | "TEXT")[];
      temperature?: number;
    };
    systemInstruction?: {
      parts: { text: string }[];
    };
    sessionResumption?: {
      handle?: string;
    };
    contextWindowCompression?: {
      /** slidingWindow keeps the session alive past the 2-min cap. */
      slidingWindow: {
        /** Target token count for the compressed window. ~128k ≈ 85 min of audio. */
        targetTokens?: number;
      };
    };
    /** Request text transcription of the model's audio output. */
    outputAudioTranscription?: Record<string, never>;
  };
}

export interface RealtimeInputAudio {
  realtimeInput: {
    audio: {
      data: string; // base64 PCM16LE 16kHz
      mimeType: "audio/pcm;rate=16000";
    };
  };
}

export interface RealtimeInputVideo {
  realtimeInput: {
    video: {
      data: string; // base64 JPEG
      mimeType: "image/jpeg";
    };
  };
}

/**
 * Text message sent via `clientContent` — the correct method for the Gemini API endpoint.
 *
 * The Google GenAI SDK is explicit: `sendRealtimeInput` handles audio/video blobs only.
 * Text ("anything that can't be represented as a Blob") always uses `sendClientContent`
 * → `clientContent` wire key with `turns` + `turnComplete: true`.
 *
 * Note: `realtimeInput.text` exists on the Vertex AI endpoint, NOT on the public
 * Gemini API (generativelanguage.googleapis.com). Using it there causes close code 1007.
 */
export interface ClientContentMessage {
  clientContent: {
    turns: {
      role: "user";
      parts: { text: string }[];
    }[];
    turnComplete: boolean;
  };
}

/** A single server message (BidiGenerateContentServerContent). */
export interface ServerMessage {
  setupComplete?: Record<string, unknown>;
  serverContent?: {
    modelTurn?: {
      parts?: Array<{
        text?: string;
        inlineData?: {
          mimeType: string;
          data: string;
        };
      }>;
    };
    /** Text transcript of the model's audio output (requires outputAudioTranscription in setup). */
    outputTranscription?: {
      text?: string;
    };
    turnComplete?: boolean;
    generationComplete?: boolean;
    interrupted?: boolean;
  };
  sessionResumptionUpdate?: {
    newHandle?: string;
    resumable?: boolean;
  };
  goAway?: {
    timeLeft?: string; // e.g. "9s"
  };
}

export type ConnectionStatus =
  | "idle"
  | "connecting"
  | "connected"
  | "reconnecting"
  | "closed"
  | "error";
