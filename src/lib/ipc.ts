import { invoke } from "@tauri-apps/api/core";
import type {
  AccountView,
  AppInfo,
  AssistantConnection,
  AssistantFinished,
  AssistantTurn,
  Balance,
  ClientKind,
  DiscoveryFeed,
  DiscoverySearch,
  FlagCatalog,
  FlagDiff,
  FlagHit,
  FlagLease,
  FlagRow,
  FlagSnapshot,
  FlagStore,
  FlagsView,
  GameDetail,
  GameServers,
  GpuPin,
  HandoverState,
  ImportedFlags,
  InstallMethod,
  LaunchReading,
  LaunchState,
  LaunchTile,
  Launched,
  Ledger,
  LinkState,
  MultiInstance,
  Party,
  PlayLog,
  Played,
  PresenceView,
  PrunePlan,
  PruneResult,
  QuickLoginAttempt,
  QuickLoginPoll,
  Receipt,
  Roster,
  Settings,
  Shelf,
  Squad,
  StagePlace,
  StageView,
  StripState,
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
  robux: () => call<Balance>("robux_balance"),
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
  /** A Play press in this window: the launcher stays where it is. */
  launchLink: (target: string, userId: number | null) =>
    call<Launched>("launch_link", { target, userId }),
  launchStart: (target: string, userId: number | null) =>
    call<Launched>("launch_start", { target, userId }),
  launchMultiInstance: (enabled: boolean) =>
    call<MultiInstance>("launch_multi_instance", { enabled }),
  launchState: () => call<LaunchState>("launch_state"),

  /** Tiles: a destination that remembers its account, flags and build. */
  tilesList: () => call<LaunchTile[]>("profiles_list"),
  tileSave: (profile: LaunchTile) => call<LaunchTile[]>("profiles_save", { profile }),
  tileDelete: (id: string) => call<LaunchTile[]>("profiles_delete", { id }),
  tileLaunch: (id: string) => call<Launched>("profiles_launch", { id }),

  /** Whether Discord is being told anything, and what it would show. */
  presenceView: () => call<PresenceView>("presence_view"),
  presenceSave: (enabled: boolean, appId: string) =>
    call<PresenceView>("presence_save", { enabled, appId }),

  /** Where `kuro://` points right now, and whether that is this build. */
  linkState: () => call<LinkState>("link_state"),
  linkRegister: () => call<LinkState>("link_register"),
  linkUnregister: () => call<LinkState>("link_unregister"),
  /** The link waiting for the page, taken rather than read. */
  linkTake: () => call<Party | null>("link_take"),
  nativeTake: () => call<string | null>("native_take"),
  handoverState: () => call<HandoverState>("handover_state"),
  handoverTake: () => call<HandoverState>("handover_take"),
  handoverGiveBack: () => call<HandoverState>("handover_give_back"),
  /** The text for a party link; null when there is nothing to point at. */
  linkFor: (place: number | null, server: string | null, account: number | null) =>
    call<string | null>("link_for", { place, server, account }),

  /** What Windows says about the graphics preference for the build about to run. */
  gpuView: () => call<GpuPin>("gpu_view"),
  /** Writes the entry now, and remembers whether launches re-apply it. */
  gpuPin: (on: boolean) => call<GpuPin>("gpu_pin", { on }),

  /** The session in progress, for the window that floats over the game. */
  stripState: () => call<StripState>("strip_state"),
  stripOpen: () => call<void>("strip_open"),
  stripClose: () => call<void>("strip_close"),

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

  /** Every client launch this machine still has a log for, newest first. */
  receiptsScan: () => call<Receipt[]>("receipts_scan"),
  /** Playtime the log folder cannot hold any more, with its coverage stated. */
  ledgerView: () => call<Ledger>("ledger_view"),
  /** Place ids to names, best effort, cached on disk after the first answer. */
  receiptNames: (places: number[]) =>
    call<Record<string, string>>("receipts_names", { places }),
  /** Opens one client log in whatever the machine uses for .log files. */
  receiptsOpen: (path: string) => call<void>("receipts_open", { path }),

  /** The friend list with names, faces and live presence. Needs an account. */
  friendsRoster: () => call<Roster>("friends_roster"),

  /** The pads: every account on this machine, its figure, and what it is doing. */
  stageView: () => call<StageView>("stage_view"),
  /** Places this machine has already named. Read from disk, no network. */
  stagePlaces: () => call<StagePlace[]>("stage_places"),
  /** Hands one destination to every pad named, and reports each one. */
  stageLaunch: (target: string, accounts: number[]) =>
    call<Squad>("stage_launch", { target, accounts }),

  /** The endpoint, the model, and whether a key is set. Never the key. */
  assistantConfig: () => call<AssistantConnection>("assistant_config"),
  assistantModels: () => call<string[]>("assistant_models"),
  /** `key: null` keeps the stored key; `""` clears it. */
  assistantSave: (input: { baseUrl: string; model: string; key: string | null }) =>
    call<AssistantConnection>("assistant_save", { input }),
  /** Streams on the `assistant:delta` event; resolves when the answer ends. */
  assistantSend: (turns: AssistantTurn[], context: string) =>
    call<AssistantFinished>("assistant_send", { request: { turns, context } }),
  assistantStop: () => call<void>("assistant_stop"),

  flagsView: () => call<FlagsView>("flags_view"),
  flagsRefresh: () => call<FlagCatalog>("flags_refresh"),
  /** Catalogues kept so far, newest first. */
  flagsSnapshots: () => call<FlagSnapshot[]>("flags_snapshots"),
  /** What moved between two stored catalogues. */
  flagsDiff: (from: string, to: string) => call<FlagDiff>("flags_diff", { from, to }),
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
