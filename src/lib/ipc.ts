import { invoke } from "@tauri-apps/api/core";
import type { AppInfo, Settings } from "./types";

export type FailureCode =
  | "client_missing"
  | "client_corrupt"
  | "session_expired"
  | "session_flagged"
  | "ticket_throttled"
  | "mutex_held"
  | "already_running"
  | "queue_full"
  | "flag_rejected"
  | "early_exit"
  | "network"
  | "update_in_flight"
  | "config_unreadable"
  | "config_unwritable"
  | "not_found"
  | "internal";

export type Failure = {
  code: FailureCode;
  message: string;
  action?: string;
};

/**
 * Every command rejects with the Rust failure taxonomy, never with a bare
 * string. Screens render `message` and offer `action`; `code` is the contract.
 */
export class CommandError extends Error {
  readonly code: FailureCode;
  readonly action?: string;

  constructor(failure: Failure) {
    super(failure.message);
    this.name = "CommandError";
    this.code = failure.code;
    this.action = failure.action;
  }
}

function isFailure(value: unknown): value is Failure {
  return (
    typeof value === "object" &&
    value !== null &&
    "code" in value &&
    "message" in value &&
    typeof (value as Failure).message === "string"
  );
}

function toCommandError(value: unknown): CommandError {
  if (value instanceof CommandError) return value;
  if (isFailure(value)) return new CommandError(value);
  if (typeof value === "string") {
    return new CommandError({ code: "internal", message: value });
  }
  return new CommandError({
    code: "internal",
    message: "The app failed without saying why. That is a bug, not your fault.",
  });
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (value) {
    throw toCommandError(value);
  }
}

export const api = {
  appInfo: () => call<AppInfo>("app_info"),
  loadSettings: () => call<Settings>("settings_load"),
  saveSettings: (next: Settings) => call<Settings>("settings_save", { next }),
  openDataRoot: () => call<void>("open_data_root"),
};
