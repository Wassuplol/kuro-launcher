import { toast } from "sonner";
import { create } from "zustand";

import { api, CommandError, failureDetail } from "../lib/ipc";
import type { AccountView } from "../lib/types";

function announce(value: unknown, fallback: string): void {
  const failure = value instanceof CommandError ? value : null;
  toast.error(failure?.message ?? fallback, { description: failureDetail(failure) });
}

type AccountsState = {
  accounts: AccountView[] | null;
  loading: boolean;
  adding: boolean;
  checking: boolean;

  load: () => Promise<void>;
  add: (cookie: string) => Promise<boolean>;
  finishQuickLogin: (code: string, privateKey: string) => Promise<boolean>;
  check: () => Promise<void>;
  forget: (userId: number) => Promise<void>;
  makeDefault: (userId: number) => Promise<void>;
};

export const useAccounts = create<AccountsState>()((set) => ({
  accounts: null,
  loading: false,
  adding: false,
  checking: false,

  load: async () => {
    set({ loading: true });
    try {
      set({ accounts: await api.accountsList(), loading: false });
    } catch (value) {
      set({ loading: false, accounts: [] });
      announce(value, "Your saved accounts could not be read.");
    }
  },

  // Returns true only when the account was stored, so the field can clear
  // itself on success and keep what was pasted on failure.
  add: async (cookie) => {
    set({ adding: true });
    try {
      const accounts = await api.accountsImport(cookie);
      set({ accounts, adding: false });
      const added = accounts.find((account) => account.health === "ok");
      toast.success(added ? `Added ${added.display}.` : "Account added.");
      return true;
    } catch (value) {
      set({ adding: false });
      announce(value, "That session was not added.");
      return false;
    }
  },

  // Quick Login ends here: the exchange, the verification and the seal all
  // happen inside one Rust call, so this only stores the answer it gets back.
  finishQuickLogin: async (code, privateKey) => {
    try {
      const accounts = await api.quickLoginFinish(code, privateKey);
      set({ accounts });
      const added = accounts.find((account) => account.userId > 0);
      toast.success(added ? `Signed in as ${added.display}.` : "Signed in.");
      return true;
    } catch (value) {
      announce(value, "The approval could not be turned into a session.");
      return false;
    }
  },

  check: async () => {
    set({ checking: true });
    try {
      set({ accounts: await api.accountsCheck(), checking: false });
    } catch (value) {
      set({ checking: false });
      announce(value, "The sessions could not be checked.");
    }
  },

  forget: async (userId) => {
    try {
      set({ accounts: await api.accountsRemove(userId) });
      toast.success("Forgotten on this machine. The session is still open on Roblox.");
    } catch (value) {
      announce(value, "That account was not forgotten.");
    }
  },

  makeDefault: async (userId) => {
    try {
      set({ accounts: await api.accountsDefault(userId) });
    } catch (value) {
      announce(value, "The default account was not changed.");
    }
  },
}));
