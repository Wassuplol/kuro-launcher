import { invoke } from "@tauri-apps/api/core";
import type {
  AccountView,
  AppInfo,
  ClientKind,
  FlagsView,
  FlagCatalog,
  FlagHit,
  FlagLease,
  FlagRow,
  FlagStore,
  DiscoveryFeed,
  DiscoverySearch,
  GameDetail,
  GameServers,
  ImportedFlags,
  InstallMethod,
  Launched,
  LaunchReading,
  LaunchState,
  MultiInstance,
  PlayLog,
  Played,
  PrunePlan,
  PruneResult,
  QuickLoginAttempt,
  QuickLoginPoll,
  Settings,
  Shelf,
  SupplySnapshot,
  VerifyReport,
} from "./types";

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
  | "channel_unavailable"
  | "cancelled"
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
  resetSettings: () => call<Settings>("settings_reset"),
  openDataRoot: () => call<void>("open_data_root"),
  openVersionsRoot: () => call<void>("open_versions_root"),

  accountsList: () => call<AccountView[]>("accounts_list"),
  /** The cookie goes to Rust and stops there; nothing comes back but accounts. */
  accountsImport: (cookie: string) => call<AccountView[]>("accounts_import", { cookie }),
  accountsCheck: () => call<AccountView[]>("accounts_check"),
  accountsRemove: (userId: number) => call<AccountView[]>("accounts_remove", { userId }),
  accountsDefault: (userId: number) => call<AccountView[]>("accounts_default", { userId }),

  /** Roblox's own cross-device sign-in: a code, a QR, a poll, an exchange. */
  quickLoginStart: () => call<QuickLoginAttempt>("quicklogin_start"),
  quickLoginPoll: (code: string, privateKey: string) =>
    call<QuickLoginPoll>("quicklogin_poll", { code, privateKey }),
  quickLoginCancel: (code: string, privateKey: string) =>
    call<void>("quicklogin_cancel", { code, privateKey }),
  /** Comes back as accounts only: the cookie is sealed before this resolves. */
  quickLoginFinish: (code: string, privateKey: string) =>
    call<AccountView[]>("quicklogin_finish", { code, privateKey }),

  /** Opens a Roblox page in the default browser. Anything else is refused. */
  openSite: (url: string) => call<void>("open_site", { url }),

  /** Reads a destination without starting anything. */
  launchRead: (target: string) => call<LaunchReading>("launch_read", { target }),
  launchStart: (target: string, userId: number | null) =>
    call<Launched>("launch_start", { target, userId }),
  launchMultiInstance: (enabled: boolean) =>
    call<MultiInstance>("launch_multi_instance", { enabled }),
  launchState: () => call<LaunchState>("launch_state"),

  discoveryHome: () => call<DiscoveryFeed>("discovery_home"),
  discoverySearch: (term: string, people: boolean) =>
    call<DiscoverySearch>("discovery_search", { term, people }),
  discoveryBudget: () => call<[string, number, number][]>("discovery_budget"),
  discoveryGame: (universe: number) => call<GameDetail>("discovery_game", { universe }),
  discoveryServers: (place: number) => call<GameServers>("discovery_servers", { place }),

  /** One page of the account's favourites. Public data: no session needed. */
  libraryFavorites: (userId: number, cursor: string | null) =>
    call<Shelf>("library_favorites", { userId, cursor }),
  /** What this machine launched, newest first. */
  libraryRecent: () => call<PlayLog>("library_recent"),
  libraryRecord: (entry: Omit<Played, "at">) => call<PlayLog>("library_record", { entry }),
  libraryClear: () => call<PlayLog>("library_clear"),

  flagsView: () => call<FlagsView>("flags_view"),
  flagsRefresh: () => call<FlagCatalog>("flags_refresh"),
  flagsSearch: (query: string, limit?: number) =>
    call<FlagHit[]>("flags_search", { query, limit }),
  flagsSaveProfile: (name: string, values: Record<string, unknown>) =>
    call<FlagStore>("flags_save_profile", { name, values }),
  flagsDeleteProfile: (name: string) => call<FlagStore>("flags_delete_profile", { name }),
  flagsSetScope: (slot: "global" | "account" | "game", id: number | null, profile: string | null) =>
    call<FlagStore>("flags_set_scope", { slot, id, profile }),
  flagsResolve: (account: number | null, place: number | null) =>
    call<FlagRow[]>("flags_resolve", { account, place }),
  flagsApply: (account: number | null, place: number | null) =>
    call<FlagLease>("flags_apply", { account, place }),
  flagsRevert: () => call<string | null>("flags_revert"),
  flagsExport: (name: string) => call<string>("flags_export", { name }),
  flagsImport: (path: string) => call<ImportedFlags>("flags_import", { path }),

  /** Shrinks and shows the window at the opening scene's size. */
  windowOpen: (width: number, height: number) =>
    call<void>("window_open", { width, height }),
  /** Grows the window to the app's size around a fixed centre. */
  windowGrow: (width: number, height: number, durationMs: number) =>
    call<void>("window_grow", { width, height, durationMs }),

  supplySnapshot: () => call<SupplySnapshot>("supply_snapshot"),
  supplyRefresh: () => call<SupplySnapshot>("supply_refresh"),
  supplyInstall: (kind: ClientKind, upload: string, method: InstallMethod) =>
    call<SupplySnapshot>("supply_install", { kind, upload, method }),
  supplyCancel: () => call<void>("supply_cancel"),
  supplyPin: (folder: string | null) => call<Settings>("supply_pin", { folder }),
  supplyPrunePlan: () => call<PrunePlan>("supply_prune_plan"),
  supplyPrune: () => call<PruneResult>("supply_prune"),
  supplyVerify: (folder: string) => call<VerifyReport>("supply_verify", { folder }),
  supplyRepair: (folder: string) => call<VerifyReport>("supply_repair", { folder }),
  supplyRemove: (folder: string) => call<SupplySnapshot>("supply_remove", { folder }),
};

/** Every failure reaches the screen as the code plus the offered action. */
export function detailOf(code: string, action?: string): string {
  return action ? `${code} · ${action}` : code;
}

export function failureDetail(failure: CommandError | null): string | undefined {
  if (!failure) return undefined;
  return detailOf(failure.code, failure.action);
}
