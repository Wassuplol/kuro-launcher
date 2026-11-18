import { listen } from "@tauri-apps/api/event";
import { create } from "zustand";

import { api, CommandError, detailOf } from "../lib/ipc";
import type { AssistantConnection, AssistantTurn } from "../lib/types";
import { useAccounts } from "./accounts";
import { useLibrary } from "./library";
import { useShell } from "./shell";

/**
 * The assistant's conversation.
 *
 * Streaming is an event, not a return value: Rust emits `assistant:delta` while the
 * answer is being written and resolves the command when it ends, so the pane paints
 * text as it arrives instead of holding a spinner for ten seconds. Nothing is on a
 * timer and nothing is sent twice: one answer in flight, one flag to stop it.
 *
 * History is this session only. A conversation that is not persisted cannot leak
 * an account name into a file, and the pane says what it remembers by simply
 * showing it.
 */
const MAX_TURNS = 12;

type AssistantState = {
  connection: AssistantConnection | null;
  loaded: boolean;
  turns: AssistantTurn[];
  /** The answer being written right now, before it becomes a turn. */
  draft: string;
  streaming: boolean;
  problem: { message: string; detail?: string } | null;

  load: () => Promise<void>;
  setConnection: (connection: AssistantConnection) => void;
  send: (text: string) => Promise<void>;
  stop: () => Promise<void>;
  clear: () => void;
};

let wired = false;

async function wire(): Promise<void> {
  if (wired) return;
  wired = true;
  await listen<{ text: string }>("assistant:delta", (event) => {
    useAssistant.setState((state) => ({ draft: state.draft + event.payload.text }));
  });
}

/** What the model is told about the app, gathered at send time so it is current. */
async function context(): Promise<string> {
  const shell = useShell.getState();

  // Load what the context needs instead of reading a store that may never have been
  // filled. The first version read `accounts` straight from the store, got null on a
  // page that does not need accounts, and told the person they had none saved.
  if (useAccounts.getState().accounts === null) {
    await useAccounts.getState().load().catch(() => undefined);
  }
  const accounts = useAccounts.getState().accounts ?? [];

  if (!useLibrary.getState().historyLoaded) {
    await useLibrary.getState().loadHistory().catch(() => undefined);
  }
  const played = useLibrary.getState().history.slice(0, 3);

  let client = "unknown";
  try {
    client = (await api.launchState()).playerRunning ? "a client is running" : "no client is running";
  } catch {
    client = "the client state could not be read";
  }

  return [
    `Screen: ${shell.page}`,
    `Accounts: ${
      accounts.length
        ? accounts
            .map(
              (account) =>
                `${account.display}${account.isDefault ? " (default)" : ""} ${account.health}`,
            )
            .join(", ")
        : "none saved"
    }`,
    `Client: ${client}`,
    `Launched from here recently: ${
      played.length ? played.map((entry) => entry.name ?? `place ${entry.place}`).join(", ") : "nothing"
    }`,
  ].join("\n");
}

export const useAssistant = create<AssistantState>()((set, get) => ({
  connection: null,
  loaded: false,
  turns: [],
  draft: "",
  streaming: false,
  problem: null,

  load: async () => {
    await wire();
    if (get().loaded) return;
    try {
      set({ connection: await api.assistantConfig(), loaded: true });
    } catch (value) {
      const failure = value instanceof CommandError ? value : null;
      set({
        loaded: true,
        problem: {
          message: failure?.message ?? "The assistant could not read its settings.",
          detail: failure ? detailOf(failure.code, failure.action) : undefined,
        },
      });
    }
  },

  setConnection: (connection) => set({ connection, loaded: true }),

  send: async (text) => {
    const content = text.trim();
    if (!content || get().streaming) return;

    await wire();
    const turns: AssistantTurn[] = [...get().turns, { role: "user", content }];
    set({ turns, draft: "", streaming: true, problem: null });

    try {
      const known = await context();
      await api.assistantSend(turns.slice(-MAX_TURNS), known);

      const reply = get().draft.trim();
      set((state) => ({
        turns: reply ? [...state.turns, { role: "assistant", content: reply }] : state.turns,
        draft: "",
        streaming: false,
      }));
    } catch (value) {
      const failure = value instanceof CommandError ? value : null;
      set({
        draft: "",
        streaming: false,
        problem: {
          message: failure?.message ?? "The assistant did not answer.",
          detail: failure ? detailOf(failure.code, failure.action) : undefined,
        },
      });
    }
  },

  stop: async () => {
    try {
      await api.assistantStop();
    } catch {
      // The stop is best effort and the stream ends on its own either way.
    }
    set({ streaming: false });
  },

  clear: () => set({ turns: [], draft: "", problem: null }),
}));
