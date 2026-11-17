export type Intent =
  | "launch_faster"
  | "manage_accounts"
  | "tune_performance"
  | "building_something";

export type Identity = {
  name: string;
  genres: string[];
  intent: Intent | null;
  onboarded: boolean;
};

export type Settings = {
  schema: number;
  identity: Identity;
  intro: { instantStart: boolean };
  supply: { pinned: string | null };
  launch: { multiInstance: boolean };
  advanced: boolean;
};

export type SettingsError = {
  code: string;
  message: string;
  action?: string;
};

export type AppInfo = {
  name: string;
  version: string;
  identifier: string;
  dataRoot: string;
  settingsError?: SettingsError;
};

export type ClientKind = "player" | "studio";

export type InstallMethod = "bootstrap" | "direct";

/** One channel, and whether Roblox will answer it without a session. */
export type Channel = {
  id: string;
  label: string;
  available: boolean;
  note?: string | null;
};

export type LiveVersion = {
  channel: string;
  version: string;
  /** The deployment folder name, also the CDN prefix. */
  upload: string;
  /** Unix seconds, so the screen can print when it was asked. */
  checkedAt: number;
};

export type InstalledClient = {
  folder: string;
  path: string;
  /** Which root this folder sits in, in words. */
  root: string;
  perUser: boolean;
  kind: ClientKind | null;
  version: string | null;
  publisher: string | null;
  /** The executable is present and its own version resource names Roblox. */
  real: boolean;
  bytes: number;
  files: number;
  modified: number | null;
  /** Kuro installed this folder, so it has hashes to check against. */
  tracked: boolean;
  pinned: boolean;
  /** The process holding this folder open, when one does. */
  holder: string | null;
  live: boolean;
};

export type ClientRoot = {
  path: string;
  label: string;
  perUser: boolean;
  exists: boolean;
};

export type SupplySnapshot = {
  /** Where a new client would be written. */
  versionsRoot: string;
  roots: ClientRoot[];
  exists: boolean;
  installed: InstalledClient[];
  livePlayer: LiveVersion | null;
  liveStudio: LiveVersion | null;
  channels: Channel[];
  totalBytes: number;
};

export type InstallProgress = {
  phase: string;
  label: string;
  done: number;
  total: number;
};

export type PrunePlan = {
  remove: string[];
  keep: { folder: string; reason: string }[];
  bytes: number;
};

export type PruneResult = {
  removed: string[];
  freed: number;
  kept: { folder: string; reason: string }[];
  failed: { folder: string; code: string; message: string }[];
};

export type VerifyReport = {
  folder: string;
  /** False means the check could not run, not that the folder is clean. */
  tracked: boolean;
  checked: number;
  mismatched: string[];
  missing: string[];
  packages: string[];
};

export type AccountHealth = "ok" | "expired" | "unknown";

/** An account as the screen sees it. There is no cookie in this shape. */
export type AccountView = {
  userId: number;
  name: string;
  display: string;
  added: number;
  health: AccountHealth;
  checkedAt: number;
  isDefault: boolean;
  isLastUsed: boolean;
};

/** Where a Quick Login attempt has got to. Roblox's own names, mapped once. */
export type QuickLoginState =
  | "created"
  | "user_linked"
  | "validated"
  | "cancelled"
  | "timed_out"
  | "unknown";

export type QuickLoginAttempt = {
  code: string;
  privateKey: string;
  expiresAt: string;
  expiresIn: number;
  /** A data URL, or empty when the picture could not be fetched. */
  qr: string;
};

export type QuickLoginPoll = {
  state: QuickLoginState;
  accountName: string | null;
  accountPicture: string | null;
  expiresAt: string | null;
};

/** Where a launch is pointed. Roblox's five destinations, named once. */
export type LaunchMode = "normal" | "server" | "private" | "follow";

export type LaunchReading = {
  mode: LaunchMode;
  modeLabel: string;
  place: number | null;
  server: string | null;
  client: string | null;
  version: string | null;
  summary: string;
};

export type Launched = {
  mode: LaunchMode;
  modeLabel: string;
  place: number | null;
  server: string | null;
  account: string;
  client: string;
  version: string | null;
  /** True when the same request was answered moments ago and nothing spawned. */
  reused: boolean;
  detail: string;
};

export type LaunchState = {
  /** A game client is running right now. */
  playerRunning: boolean;
  multiInstance: boolean;
};

export type MultiInstance = {
  settings: Settings;
  /** False when another program already held the lock, so it applies next start. */
  created: boolean;
};
