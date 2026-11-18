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
  launch: { multiInstance: boolean; gpuPin: boolean };
  discord: { enabled: boolean; appId: string };
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
  /** What happened to the flag file on this launch, when it changed. */
  flags?: string | null;
  /** What happened to the graphics preference on this launch, when it changed. */
  gpu?: string | null;
};

/**
 * One saved launch: a destination plus the answers that would otherwise be picked
 * again. `flags` and `client` are names, not copies, so editing those changes what a
 * tile launches with.
 */
export type LaunchTile = {
  id: string;
  name: string;
  target: string;
  account: number | null;
  flags: string | null;
  client: string | null;
  after: "stay" | "minimize";
  /** Unix seconds of the last launch, and how many times it has run. */
  at: number;
  used: number;
};

/** What the settings row says about Discord presence. */
export type PresenceView = {
  enabled: boolean;
  appId: string;
  status: {
    connected: boolean;
    /** What a friend would see right now. */
    line: string | null;
    error: string | null;
  };
  discordRunning: boolean;
};

/** What a `kuro://` link carries. Every field can be missing: a link is text from outside. */
export type Party = {
  place: number | null;
  /** The job id: Roblox's own name for one exact server instance. */
  server: string | null;
  account: number | null;
};

/** Whether `kuro://` points at this build. */
export type LinkState = {
  command: string | null;
  ours: boolean;
  exe: string;
};

export type LedgerTotals = {
  seconds: number;
  sessions: number;
  /** Places played, not launches made. */
  places: number;
  days: number;
};

export type LedgerPlace = {
  place: number;
  name: string | null;
  seconds: number;
  sessions: number;
};

/** Totals that outlive the log folder, plus the card the Sessions page draws. */
export type Ledger = {
  all: LedgerTotals;
  year: number;
  thisYear: LedgerTotals;
  /** The first and last session the ledger holds, unix seconds. */
  first: number | null;
  last: number | null;
  top: LedgerPlace[];
  yearTop: LedgerPlace[];
  longest: { place: number | null; name: string | null; seconds: number; started: number } | null;
  busiest: { date: string; seconds: number } | null;
};

export type StripState = {
  running: boolean;
  place: number | null;
  name: string | null;
  /** `<ip>:<port>` from the client's handshake line, once one has happened. */
  endpoint: string | null;
  account: string | null;
  /** The client's own clock at the last read, in seconds. */
  seconds: number;
  ping: number | null;
  /** Why there is no ping number, said once instead of left blank. */
  note: string | null;
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

/**
 * The per-program graphics preference Windows keeps for this build.
 *
 * `current` is Windows' own name for the entry, `pinned` is whether the launch path
 * re-applies it. They can disagree for one launch: a new version folder arrives with
 * no entry, and the switch writes it on the way in.
 */
export type GpuPin = {
  folder: string | null;
  exe: string | null;
  current: string | null;
  pinned: boolean;
};

/** A flag's value is whatever its name promises: bool, int, number or text. */
export type FlagKind = "flag" | "int" | "string" | "log" | "number" | "unknown";

export type FlagHit = {
  key: string;
  /** Absent when Roblox does not serve the key, which means no known default. */
  default: string | null;
  kind: FlagKind;
  /**
   * Whether the client takes this key from a local file at all. Roblox allowlists
   * local overrides, so false means the key is written and then discarded.
   */
  honoured: boolean;
};

export type FlagSource = "launch" | "account" | "game" | "global";

export type FlagRow = {
  key: string;
  value: unknown;
  source: FlagSource;
  profile: string;
  kind: FlagKind;
};

export type FlagProfile = {
  name: string;
  notes: string;
  values: Record<string, unknown>;
};

export type FlagScopes = {
  global: string | null;
  accounts: Record<string, string>;
  games: Record<string, string>;
};

export type FlagStore = {
  schema: number;
  profiles: FlagProfile[];
  scopes: FlagScopes;
};

export type FlagCatalog = {
  fetched: number;
  application: string;
  values: Record<string, string>;
};

export type FlagTarget = {
  path: string;
  exists: boolean;
  /** What is already in that file, whoever put it there. */
  current: Record<string, unknown>;
  client: string | null;
};

export type FlagLease = {
  at: number;
  path: string;
  keys: number;
  backup: string | null;
  hash: string;
};

export type FlagExport = { name: string; path: string; keys: number };

export type FlagsView = {
  catalog: FlagCatalog | null;
  store: FlagStore;
  target: FlagTarget | null;
  lease: FlagLease | null;
  exports: FlagExport[];
  recovered: string | null;
  /** The keys the client will take from a local file, as Roblox publishes them. */
  allowlist: string[];
  allowlistSource: string;
  playerRunning: boolean;
};

/** One stored flag catalogue, named by the client version it was served for. */
export type FlagSnapshot = {
  version: string;
  at: number;
  keys: number;
};

export type FlagChange = {
  key: string;
  before: string | null;
  after: string | null;
};

/** What moved between two stored catalogues. Counts are the full numbers, lists capped. */
export type FlagDiff = {
  from: FlagSnapshot;
  to: FlagSnapshot;
  added: FlagChange[];
  removed: FlagChange[];
  changed: FlagChange[];
  addedCount: number;
  removedCount: number;
  changedCount: number;
  truncated: boolean;
};

export type ImportedFlags = {
  name: string;
  values: Record<string, unknown>;
  refused: string[];
};

/** One game in a grid. `place` is what a launch needs; `universe` is the id. */
/** What one friend is doing: 0 offline, 1 online, 2 in a game, 3 in Studio. */
export type Presence = {
  kind: number;
  label: string;
  place: number | null;
  universe: number | null;
  /** The exact server, when they are in one. */
  server: string | null;
};

export type Friend = {
  user: number;
  name: string;
  display: string;
  verified: boolean;
  headshot: string | null;
  presence: Presence;
};

export type Roster = {
  friends: Friend[];
  inGame: number;
  online: number;
  offline: number;
  fetched: number;
  cached: boolean;
  note?: string;
};

/** One page of Roblox favourites, as the Library draws it. */
export type Shelf = {
  games: GameCard[];
  /** The cursor for the next page, or null on the last one. */
  next: string | null;
  fetched: number;
  cached: boolean;
  /** Something inside the page failed to load; the tiles still stand. */
  note?: string;
};

/** One launch this machine actually started. */
export type Played = {
  at: number;
  place: number;
  universe?: number | null;
  name?: string | null;
  mode: string;
  account?: number | null;
};

export type PlayLog = {
  schema: number;
  entries: Played[];
};

export type GameCard = {
  universe: number;
  place: number;
  name: string;
  players: number;
  creator: string;
  icon: string | null;
};

export type FeedSection = {
  topic: string;
  treatment: string;
  games: GameCard[];
};

export type DiscoveryFeed = {
  sections: FeedSection[];
  fetched: number;
  /** True when this is a cached answer, which the page says rather than hiding. */
  cached: boolean;
};

export type Person = {
  user: number;
  name: string;
  display: string;
  verified: boolean;
  headshot: string | null;
};

export type DiscoverySearch = {
  games: GameCard[];
  people: Person[];
  fetched: number;
  cached: boolean;
};

export type GameDetail = {
  universe: number;
  place: number;
  name: string;
  creator: string;
  description: string;
  players: number;
  visits: number;
  maxPlayers: number;
  genre: string;
  created: string;
  updated: string;
  upVotes: number;
  downVotes: number;
  icon: string | null;
  fetched: number;
  cached: boolean;
};

export type GameServer = {
  id: string;
  playing: number;
  maxPlayers: number;
  ping: number;
  fps: number;
};

export type GameServers = {
  place: number;
  list: GameServer[];
  fetched: number;
  cached: boolean;
};

/** The assistant's connection, as the screen is allowed to know it. */
export type AssistantConnection = {
  baseUrl: string;
  model: string;
  /** Whether a key is stored. The key itself never leaves Rust. */
  keySet: boolean;
  /** "saved", "environment", "hermes" or "none", so the screen can say where it came from. */
  source: string;
};

export type AssistantTurn = {
  role: "user" | "assistant";
  content: string;
};

export type AssistantFinished = {
  ms: number;
  chars: number;
};

/**
 * One launch, as the client's own log recorded it. Every field is either written by
 * the client or absent; nothing here is inferred from the website.
 */
export type Adapter = {
  name: string;
  vramMb: number | null;
  /** The one the client actually used. */
  chosen: boolean;
};

export type Receipt = {
  id: string;
  version: string;
  /** Unix seconds, from the stamp in the log file name. */
  started: number;
  /** How long the client was alive, from the log's own clock. */
  seconds: number;
  place: number | null;
  /** Filled from this app's own history when it knows the place. */
  name: string | null;
  universe: number | null;
  job: string | null;
  server: string | null;
  /** `<ip>:<port>` from the handshake line: the server that answered. */
  endpoint: string | null;
  joinSeconds: number | null;
  handshake: boolean;
  adapter: string | null;
  api: string | null;
  vramMb: number | null;
  /** Every GPU the client enumerated for this launch. */
  adapters: Adapter[];
  shutdownStage: string | null;
  httpErrors: number;
  lastHttp: string | null;
  outcome: "left" | "ended" | "no_handshake" | "never_joined" | "crashed";
  verdict: string;
  action: string | null;
  lines: number;
  bytes: number;
  logPath: string;
};
