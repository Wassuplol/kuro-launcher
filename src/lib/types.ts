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
