import { useEffect, useState } from "react";

import { Row, Section } from "../components/Panel";
import { api, CommandError, detailOf } from "../lib/ipc";
import { useAssistant } from "../store/assistant";

/**
 * The assistant's connection, in the settings drawer.
 *
 * The key is write-only by design: it is sealed with DPAPI before it touches the
 * disk, and the screen only ever learns whether one is set and where it came from.
 * That is the same rule the session vault follows, for the same reason.
 */
const SOURCES: Record<string, string> = {
  saved: "stored in this app, sealed with Windows DPAPI",
  environment: "read from KURO_ASSISTANT_KEY or OPENAI_API_KEY",
  hermes: "read from the Hermes .env on this machine",
  none: "no key yet",
};

export function AssistantSection() {
  const connection = useAssistant((state) => state.connection);
  const load = useAssistant((state) => state.load);
  const setConnection = useAssistant((state) => state.setConnection);

  const [baseUrl, setBaseUrl] = useState("");
  const [model, setModel] = useState("");
  /** null keeps the stored key, "" clears it, anything else replaces it. */
  const [key, setKey] = useState<string | null>(null);
  const [models, setModels] = useState<string[]>([]);
  const [busy, setBusy] = useState<"models" | "save" | null>(null);
  const [note, setNote] = useState<{ text: string; bad?: boolean } | null>(null);

  useEffect(() => {
    void load();
  }, [load]);

  useEffect(() => {
    if (!connection) return;
    setBaseUrl(connection.baseUrl);
    setModel(connection.model);
  }, [connection]);

  const readModels = async () => {
    setBusy("models");
    setNote(null);
    try {
      const list = await api.assistantModels();
      setModels(list);
      setNote({
        text: `${list.length} models. The field below accepts any of them.`,
      });
    } catch (value) {
      const failure = value instanceof CommandError ? value : null;
      setNote({
        text: failure?.message ?? "The model list could not be read.",
        bad: true,
      });
    } finally {
      setBusy(null);
    }
  };

  const save = async () => {
    setBusy("save");
    setNote(null);
    try {
      const next = await api.assistantSave({ baseUrl, model, key });
      setConnection(next);
      setKey(null);
      setNote({ text: "Saved." });
    } catch (value) {
      const failure = value instanceof CommandError ? value : null;
      setNote({
        text: failure ? `${failure.message} (${detailOf(failure.code, failure.action)})` : "The assistant settings were not saved.",
        bad: true,
      });
    } finally {
      setBusy(null);
    }
  };

  const dirty =
    Boolean(connection) &&
    (baseUrl !== connection?.baseUrl || model !== connection?.model || key !== null);

  return (
    <Section title="Assistant">
      <Row
        label="Key"
        hint={SOURCES[connection?.source ?? "none"] ?? connection?.source ?? "reading"}
      >
        <div className="flex shrink-0 items-center gap-1.5">
          <input
            type="password"
            value={key ?? ""}
            onChange={(event) => setKey(event.target.value)}
            placeholder={connection?.keySet ? "Stored. Type to replace" : "Paste a key"}
            aria-label="Assistant key"
            autoComplete="off"
            className="h-7 w-[190px] rounded-control border border-white/10 bg-page px-2 text-[12px] text-body outline-none placeholder:text-muted/70 focus:border-gold/60"
          />
          {connection?.keySet ? (
            <button
              type="button"
              onClick={() => {
                setKey("");
                setNote({ text: "Press Save to remove the stored key." });
              }}
              className="ui h-7 shrink-0 rounded-control border border-white/10 px-2 text-[11.5px] text-muted hover:border-danger/40 hover:text-danger"
            >
              Remove
            </button>
          ) : null}
        </div>
      </Row>

      <Row label="Endpoint" hint="Any OpenAI-compatible base URL. Empty uses the default.">
        <input
          value={baseUrl}
          onChange={(event) => setBaseUrl(event.target.value)}
          aria-label="Assistant endpoint"
          spellCheck={false}
          className="h-7 w-[250px] shrink-0 rounded-control border border-white/10 bg-page px-2 text-[12px] text-body outline-none placeholder:text-muted/70 focus:border-gold/60"
        />
      </Row>

      <Row label="Model" hint={models.length ? `${models.length} listed by the endpoint` : "Set the key, then list the models."}>
        <div className="flex shrink-0 items-center gap-1.5">
          <input
            value={model}
            onChange={(event) => setModel(event.target.value)}
            list="assistant-models"
            aria-label="Assistant model"
            spellCheck={false}
            className="h-7 w-[190px] rounded-control border border-white/10 bg-page px-2 text-[12px] text-body outline-none placeholder:text-muted/70 focus:border-gold/60"
          />
          <datalist id="assistant-models">
            {models.map((id) => (
              <option key={id} value={id} />
            ))}
          </datalist>
          <button
            type="button"
            onClick={() => void readModels()}
            disabled={busy !== null}
            className="ui h-7 shrink-0 rounded-control border border-white/10 px-2 text-[11.5px] text-muted hover:text-body disabled:opacity-40"
          >
            {busy === "models" ? "Reading" : "List models"}
          </button>
        </div>
      </Row>

      <div className="flex items-center gap-3 border-t border-white/6 px-3.5 py-2.5">
        <button
          type="button"
          onClick={() => void save()}
          disabled={!dirty || busy !== null}
          className="ui h-7 rounded-control bg-gold px-3 text-[12px] font-medium text-page disabled:bg-white/8 disabled:text-muted"
        >
          {busy === "save" ? "Saving" : "Save"}
        </button>
        {note ? (
          <span className={`text-[11.5px] ${note.bad ? "text-danger" : "text-muted"}`}>{note.text}</span>
        ) : null}
      </div>
    </Section>
  );
}
