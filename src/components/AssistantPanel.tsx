import { useEffect, useMemo, useRef, useState } from "react";
import { ArrowUp, Square, Trash2, X } from "lucide-react";

import type { AssistantTurn } from "../lib/types";
import { useAssistant } from "../store/assistant";
import { PAGES, PAGE_LABEL, type Page } from "../app/router";
import { useShell } from "../store/shell";

/**
 * The assistant dock: a column beside the page, not a sheet over it.
 *
 * That distinction is the whole design. A modal would interrupt the screen it is
 * answering about; this sits next to the Library or the Flags page while the person
 * reads both. Escape does not close it for the same reason: it is where the pane
 * lives, not a layer on top of where they are.
 *
 * The model's proposed actions are rendered as buttons and never executed. It may
 * suggest "open the Flags page" or "prepare place 189707"; a person clicks, and the
 * same store functions the rail uses do the work.
 */
const SUGGESTIONS = [
  "What is running right now?",
  "Which client would a launch use?",
  "How do I get the client onto Vulkan?",
];

type Action =
  | { kind: "open"; page: string }
  | { kind: "launch"; place: number };

/** Splits a finished answer into the words and the one action it proposed. */
function readReply(content: string): { text: string; action: Action | null } {
  const match = /^ACTION:\s*(\{.*\})\s*$/m.exec(content);
  if (!match) return { text: content.trim(), action: null };

  const text = content.replace(match[0], "").trim();
  try {
    const parsed = JSON.parse(match[1] ?? "{}") as { open?: string; launch?: number };
    if (typeof parsed.open === "string") return { text, action: { kind: "open", page: parsed.open } };
    if (typeof parsed.launch === "number" && Number.isFinite(parsed.launch)) {
      return { text, action: { kind: "launch", place: parsed.launch } };
    }
  } catch {
    // A malformed action is not worth an error: the words still stand.
  }
  return { text, action: null };
}

export function AssistantPanel() {
  const connection = useAssistant((state) => state.connection);
  const turns = useAssistant((state) => state.turns);
  const draft = useAssistant((state) => state.draft);
  const streaming = useAssistant((state) => state.streaming);
  const problem = useAssistant((state) => state.problem);
  const load = useAssistant((state) => state.load);
  const send = useAssistant((state) => state.send);
  const stop = useAssistant((state) => state.stop);
  const clear = useAssistant((state) => state.clear);

  const toggle = useShell((state) => state.toggleAssistant);
  const openSettings = useShell((state) => state.openSettings);
  const go = useShell((state) => state.go);
  const prefill = useShell((state) => state.prefill);

  const [text, setText] = useState("");
  const list = useRef<HTMLDivElement>(null);
  const field = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    void load();
  }, [load]);

  // Follow the answer as it is written, which is the only scroll that is not
  // fighting the person reading.
  useEffect(() => {
    const node = list.current;
    if (node) node.scrollTop = node.scrollHeight;
  }, [draft, turns.length]);

  useEffect(() => {
    field.current?.focus();
  }, []);

  const submit = () => {
    if (streaming) return;
    const value = text.trim();
    if (!value) return;
    setText("");
    void send(value);
  };

  const empty = turns.length === 0 && !draft && !streaming;

  const header = useMemo(() => {
    if (!connection) return "Assistant";
    if (!connection.keySet) return "Assistant · no key";
    return `Assistant · ${connection.model.split("/").pop() ?? connection.model}`;
  }, [connection]);

  return (
    <aside
      aria-label="Assistant"
      className="flex min-h-0 flex-col border-l border-white/8 bg-panel"
    >
      <header className="flex h-11 shrink-0 items-center gap-2 border-b border-white/8 pr-2 pl-4">
        <span className="truncate text-[12px] text-muted" title={connection?.model}>
          {header}
        </span>
        {connection && !connection.keySet ? (
          <button
            type="button"
            onClick={openSettings}
            className="ui shrink-0 rounded-control px-1.5 py-0.5 text-[11px] text-gold hover:bg-white/5"
          >
            Set a key
          </button>
        ) : null}

        <div className="ml-auto flex items-center">
          {turns.length > 0 ? (
            <button
              type="button"
              onClick={clear}
              title="Clear this conversation"
              aria-label="Clear this conversation"
              className="ui flex h-8 w-8 items-center justify-center rounded-control text-muted hover:bg-white/6 hover:text-body"
            >
              <Trash2 size={14} strokeWidth={1.6} />
            </button>
          ) : null}
          <button
            type="button"
            onClick={() => toggle(false)}
            title="Close the assistant"
            aria-label="Close the assistant"
            className="ui flex h-8 w-8 items-center justify-center rounded-control text-muted hover:bg-white/6 hover:text-body"
          >
            <X size={15} strokeWidth={1.6} />
          </button>
        </div>
      </header>

      <div ref={list} className="min-h-0 flex-1 overflow-y-auto px-4 py-3">
        {empty ? (
          <div className="pt-2">
            <p className="text-[12.5px] leading-[1.6] text-muted">
              Ask about this app or about your own setup. It can read the screen you are on,
              the accounts you saved and what you launched, and it cannot start a game.
            </p>
            <div className="mt-3 flex flex-col items-start gap-1.5">
              {SUGGESTIONS.map((suggestion) => (
                <button
                  key={suggestion}
                  type="button"
                  onClick={() => void send(suggestion)}
                  className="ui rounded-control border border-white/8 px-2.5 py-1 text-left text-[12px] text-muted hover:border-white/16 hover:text-body"
                >
                  {suggestion}
                </button>
              ))}
            </div>
          </div>
        ) : null}

        <div className="flex flex-col gap-3">
          {turns.map((turn, index) => (
            <Bubble
              key={`${turn.role}-${index}`}
              turn={turn}
              onOpen={go}
              onLaunch={(place) => prefill(String(place))}
            />
          ))}

          {streaming || draft ? (
            <div className="flex flex-col gap-1">
              <span className="text-[10px] tracking-[0.08em] text-muted uppercase">Assistant</span>
              <div className="whitespace-pre-wrap text-[12.5px] leading-[1.65] text-body">
                {draft}
                {streaming ? <span className="ml-0.5 inline-block h-3 w-[2px] animate-pulse bg-gold align-[-1px]" /> : null}
              </div>
            </div>
          ) : null}
        </div>

        {problem ? (
          <div className="mt-3 rounded-card border border-danger/35 bg-danger-fill/10 px-3 py-2">
            <div className="text-[12px] text-danger">{problem.message}</div>
            {problem.detail ? (
              <div className="mt-0.5 text-[11px] text-muted">{problem.detail}</div>
            ) : null}
          </div>
        ) : null}
      </div>

      <div className="shrink-0 border-t border-white/8 p-2.5">
        <div className="flex items-end gap-1.5 rounded-card border border-white/10 bg-page px-2 py-1.5 focus-within:border-gold/60">
          <textarea
            ref={field}
            value={text}
            onChange={(event) => setText(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter" && !event.shiftKey) {
                event.preventDefault();
                submit();
              }
            }}
            rows={2}
            placeholder="Ask about this app"
            aria-label="Message the assistant"
            className="max-h-32 min-h-[38px] flex-1 resize-none bg-transparent text-[12.5px] leading-[1.5] text-body outline-none placeholder:text-muted/70"
          />
          {streaming ? (
            <button
              type="button"
              onClick={() => void stop()}
              aria-label="Stop the answer"
              className="ui mb-0.5 flex h-7 w-7 items-center justify-center rounded-control bg-white/8 text-body hover:bg-white/14"
            >
              <Square size={11} strokeWidth={2.4} />
            </button>
          ) : (
            <button
              type="button"
              onClick={submit}
              disabled={!text.trim()}
              aria-label="Send"
              className="ui mb-0.5 flex h-7 w-7 items-center justify-center rounded-control bg-gold text-page disabled:bg-white/8 disabled:text-muted"
            >
              <ArrowUp size={14} strokeWidth={2.2} />
            </button>
          )}
        </div>
      </div>
    </aside>
  );
}

function Bubble({
  turn,
  onOpen,
  onLaunch,
}: {
  turn: AssistantTurn;
  onOpen: (page: Page) => void;
  onLaunch: (place: number) => void;
}) {
  if (turn.role === "user") {
    return (
      <div className="flex flex-col gap-1">
        <span className="text-[10px] tracking-[0.08em] text-muted uppercase">You</span>
        <div className="whitespace-pre-wrap text-[12.5px] leading-[1.65] text-body">
          {turn.content}
        </div>
      </div>
    );
  }

  const { text, action } = readReply(turn.content);
  const known = action?.kind === "open" && (PAGES as readonly string[]).includes(action.page);

  return (
    <div className="flex flex-col gap-1">
      <span className="text-[10px] tracking-[0.08em] text-muted uppercase">Assistant</span>
      <div className="whitespace-pre-wrap text-[12.5px] leading-[1.65] text-body">{text}</div>

      {action && (action.kind === "launch" || known) ? (
        <button
          type="button"
          onClick={() => {
            if (action.kind === "open") onOpen(action.page as Page);
            else onLaunch(action.place);
          }}
          className="ui mt-1 self-start rounded-control border border-gold/40 px-2.5 py-1 text-[11.5px] text-gold hover:bg-gold/10"
        >
          {action.kind === "open"
            ? PAGE_LABEL[action.page as Page]
              ? `Open ${PAGE_LABEL[action.page as Page]}`
              : `Open ${action.page}`
            : `Prepare place ${action.place}`}
        </button>
      ) : null}
    </div>
  );
}
