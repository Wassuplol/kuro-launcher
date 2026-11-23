import { motion } from "motion/react";
import { useEffect, useState } from "react";

import { cn } from "../lib/cn";
import { ENTER } from "../lib/motion";
import type { Intent } from "../lib/types";
import { useShell } from "../store/shell";

const GENRES = [
  "Obby",
  "Simulator",
  "Tycoon",
  "Fighting",
  "RPG",
  "Horror",
  "Racing",
  "Building",
];

const INTENTS: { id: Intent; label: string; note: string }[] = [
  {
    id: "launch_faster",
    label: "Launch faster",
    note: "One account at a time. Fewer panels. Advanced mode off.",
  },
  {
    id: "manage_accounts",
    label: "Manage accounts",
    note: "Accounts lead. Multi-instance stays on. Advanced mode off.",
  },
  {
    id: "tune_performance",
    label: "Tune performance",
    note: "Flag controls open first. Advanced mode off.",
  },
  {
    id: "building_something",
    label: "Building something",
    note: "Advanced mode on. Logs and passthrough visible.",
  },
];

const STEPS = [
  { title: "What should I call you?", note: "Used in the greeting. It stays on this machine." },
  { title: "What do you play most?", note: "Seeds the charts Home will load." },
  { title: "Why are you here?", note: "Sets the defaults the app opens with." },
];

type Props = { onDone: () => void };

export function Onboarding({ onDone }: Props) {
  const settings = useShell((state) => state.settings);
  const update = useShell((state) => state.update);

  const [step, setStep] = useState(0);
  const [name, setName] = useState(settings?.identity.name ?? "");
  const [genres, setGenres] = useState<string[]>(settings?.identity.genres ?? []);
  const [intent, setIntent] = useState<Intent | null>(settings?.identity.intent ?? null);
  const [busy, setBusy] = useState(false);

  const meta = STEPS[step] ?? STEPS[0]!;
  const last = step === STEPS.length - 1;

  const finish = async () => {
    if (busy) return;
    setBusy(true);
    await update((draft) => {
      draft.identity.name = name.trim();
      draft.identity.genres = genres;
      draft.identity.intent = intent;
      draft.identity.onboarded = true;
      if (intent) draft.advanced = intent === "building_something";
    });
    setBusy(false);
    onDone();
  };

  const skipStep = () => {
    if (step === 0) setName(settings?.identity.name ?? "");
    if (step === 1) setGenres(settings?.identity.genres ?? []);
    if (last) {
      void finish();
      return;
    }
    setStep(step + 1);
  };

  const next = () => {
    if (last) void finish();
    else setStep(step + 1);
  };

  const toggleGenre = (genre: string) => {
    setGenres((current) =>
      current.includes(genre) ? current.filter((item) => item !== genre) : [...current, genre],
    );
  };

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") void finish();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [finish]);

  return (
    <div className="fixed inset-0 z-[55] flex flex-col bg-page">
      <div className="flex h-11 shrink-0 items-center justify-end px-5">
        <button
          type="button"
          onClick={() => void finish()}
          className="ui text-[12px] text-muted hover:text-body"
        >
          Skip setup
        </button>
      </div>

      <div className="flex min-h-0 flex-1 items-start justify-center overflow-y-auto px-8 pt-[7vh] pb-10">
        <div className="w-full max-w-[520px]">
          <div className="flex gap-1.5" aria-hidden>
            {STEPS.map((_, index) => (
              <span
                key={index}
                className={cn(
                  "ui h-[2px] w-7 rounded-full",
                  index <= step ? "bg-gold" : "bg-white/12",
                )}
              />
            ))}
          </div>

          <motion.div
            key={step}
            initial={{ opacity: 0, y: 4 }}
            animate={{ opacity: 1, y: 0 }}
            transition={ENTER}
            className="mt-8"
          >
            <h1 className="text-[24px] font-semibold tracking-[-0.02em] text-body">
              {meta.title}
            </h1>
            <p className="mt-2 text-[13px] text-muted">{meta.note}</p>

            <div className="mt-7">
              {step === 0 ? (
                <form
                  onSubmit={(event) => {
                    event.preventDefault();
                    next();
                  }}
                >
                  <label htmlFor="onboarding-name" className="sr-only">
                    Your name
                  </label>
                  <input
                    id="onboarding-name"
                    autoFocus
                    value={name}
                    maxLength={24}
                    placeholder="your name"
                    onChange={(event) => setName(event.target.value)}
                    className="ui h-10 w-full rounded-control border border-white/12 bg-raised px-3 text-[14px] text-body placeholder:text-muted focus:border-gold/60 focus:outline-none"
                  />
                  <div className="num mt-2 text-[11px] text-muted">{name.length} / 24</div>
                </form>
              ) : null}

              {step === 1 ? (
                <div className="flex flex-wrap gap-2" role="group" aria-label="What you play most">
                  {GENRES.map((genre) => {
                    const on = genres.includes(genre);
                    return (
                      <button
                        key={genre}
                        type="button"
                        aria-pressed={on}
                        onClick={() => toggleGenre(genre)}
                        className={cn(
                          "ui h-8 rounded-control border px-3 text-[13px]",
                          on
                            ? "border-gold/50 bg-gold/10 text-gold"
                            : "border-white/10 text-muted hover:border-white/20 hover:text-body",
                        )}
                      >
                        {genre}
                      </button>
                    );
                  })}
                </div>
              ) : null}

              {step === 2 ? (
                <div className="flex flex-col gap-2" role="radiogroup" aria-label="Why you are here">
                  {INTENTS.map((option) => {
                    const on = intent === option.id;
                    return (
                      <button
                        key={option.id}
                        type="button"
                        role="radio"
                        aria-checked={on}
                        onClick={() => setIntent(option.id)}
                        className={cn(
                          "ui rounded-card border px-4 py-3 text-left",
                          on
                            ? "border-gold/50 bg-gold/[0.06]"
                            : "border-white/10 hover:border-white/20",
                        )}
                      >
                        <div className={cn("text-[13px] font-semibold", on ? "text-gold" : "text-body")}>
                          {option.label}
                        </div>
                        <div className="mt-0.5 text-[12px] leading-[1.5] text-muted">
                          {option.note}
                        </div>
                      </button>
                    );
                  })}
                </div>
              ) : null}
            </div>
          </motion.div>

          <div className="mt-10 flex items-center justify-between">
            <button
              type="button"
              onClick={skipStep}
              className="ui h-9 rounded-control px-3 text-[13px] text-muted hover:bg-white/5 hover:text-body"
            >
              Skip
            </button>
            <button
              type="button"
              onClick={next}
              disabled={busy}
              className="ui h-9 rounded-control bg-gold px-4 text-[13px] font-semibold text-page hover:bg-gold-hover disabled:opacity-60"
            >
              {last ? "Finish" : "Continue"}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
