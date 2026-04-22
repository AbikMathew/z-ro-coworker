/**
 * Session handle persistence for Gemini Live.
 *
 * Google's Live API issues a "resumption handle" periodically via
 * `sessionResumptionUpdate` messages. If we save the latest handle and
 * open a new WebSocket with `setup.sessionResumption.handle = savedHandle`,
 * the server continues the same session (valid for 2 hours after disconnect).
 *
 * We persist to localStorage so a page refresh or app restart still resumes.
 */

const STORAGE_KEY = "zro.gemini.handle";

export function loadHandle(): string | null {
  if (typeof localStorage === "undefined") return null;
  try {
    return localStorage.getItem(STORAGE_KEY);
  } catch {
    return null;
  }
}

export function saveHandle(handle: string): void {
  if (typeof localStorage === "undefined") return;
  try {
    localStorage.setItem(STORAGE_KEY, handle);
  } catch {
    // quota / private browsing — tolerable, just means no persistence
  }
}

export function clearHandle(): void {
  if (typeof localStorage === "undefined") return;
  try {
    localStorage.removeItem(STORAGE_KEY);
  } catch {
    // ignore
  }
}

/** Parse `goAway.timeLeft` strings like "9s", "1.5s" into milliseconds. */
export function parseTimeLeftMs(timeLeft: string | undefined): number {
  if (!timeLeft) return Number.POSITIVE_INFINITY;
  const match = timeLeft.match(/^([\d.]+)\s*s?$/i);
  if (!match) return Number.POSITIVE_INFINITY;
  return Math.round(parseFloat(match[1]) * 1000);
}
