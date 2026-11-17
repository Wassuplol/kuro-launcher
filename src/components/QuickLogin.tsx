import { useEffect, useRef, useState } from "react";

import { cn } from "../lib/cn";
import { api, CommandError } from "../lib/ipc";
import type { QuickLoginAttempt } from "../lib/types";
import { useAccounts } from "../store/accounts";

/**
 * Roblox's cross-device sign-in, offered before the paste field because it is
 * the path that asks for nothing: no password into this app, no cookie out of a
 * browser's developer tools.
 *
 * The page owns the attempt. Rust holds no state between calls, so closing the
 * drawer cannot leave a poll running in the background, and the pending approval
 * is cancelled on the way out rather than left live on the person's account.
 */

/** Roblox suggests about four seconds. Three keeps the approval feeling immediate. */
const POLL_MS = 3000;

type Stage = "idle" | "starting" | "waiting" | "finishing" | "done" | "cancelled" | "expired";

export function QuickLogin() {
  const finish = useAccounts((state) => state.finishQuickLogin);

  const [stage, setStage] = useState<Stage>("idle");
  const [attempt, setAttempt] = useState<QuickLoginAttempt | null>(null);
  const [remaining, setRemaining] = useState(0);
  const [problem, setProblem] = useState<string | null>(null);
  const [who, setWho] = useState<string | null>(null);

  // The cleanup below must not read a stale stage, and must not cancel an
  // attempt it already finished.
  const live = useRef<{ code: string; key: string } | null>(null);
  const finished = useRef(false);

  const forget = () => {
    live.current = null;
    setAttempt(null);
    setRemaining(0);
  };

  const start = async () => {
    setStage("starting");
    setProblem(null);
    setWho(null);
    finished.current = false;
    try {
      const next = await api.quickLoginStart();
      live.current = { code: next.code, key: next.privateKey };
      setAttempt(next);
      setRemaining(next.expiresIn);
      setStage("waiting");
    } catch (value) {
      const failure = value instanceof CommandError ? value : null;
      setProblem(failure?.message ?? "A sign-in code could not be created.");
      setStage("idle");
    }
  };

  const cancel = () => {
    const current = live.current;
    live.current = null;
    setAttempt(null);
    setRemaining(0);
    setStage("idle");
    if (current) {
      // Fire and forget: the person has already moved on, and a failed cancel
      // only means the code expires on its own.
      void api.quickLoginCancel(current.code, current.key).catch(() => undefined);
    }
  };

  // The countdown runs off Roblox's own expiry, so the number on screen is the
  // number the service enforces.
  useEffect(() => {
    if (stage !== "waiting" || !attempt) return;
    if (remaining <= 0) {
      setStage("expired");
      forget();
      return;
    }
    const timer = window.setTimeout(() => setRemaining((seconds) => seconds - 1), 1000);
    return () => window.clearTimeout(timer);
  }, [stage, attempt, remaining]);

  // One poll at a time, and only while the attempt is live.
  useEffect(() => {
    if (stage !== "waiting") return;
    const current = live.current;
    if (!current) return;

    let stop = false;
    const tick = async () => {
      if (stop || finished.current) return;
      try {
        const answer = await api.quickLoginPoll(current.code, current.key);
        if (stop) return;

        if (answer.state === "validated") {
          finished.current = true;
          setStage("finishing");
          const stored = await finish(current.code, current.key);
          if (stop) return;
          if (stored) {
            setWho(answer.accountName);
            setStage("done");
            forget();
          } else {
            // The exchange was offered and refused. Start again rather than
            // polling a code the service has already spent.
            finished.current = false;
            setStage("idle");
          }
          return;
        }

        if (answer.state === "cancelled") {
          finished.current = true;
          setStage("cancelled");
          forget();
          return;
        }

        if (answer.state === "timed_out") {
          finished.current = true;
          setStage("expired");
          forget();
          return;
        }

        if (answer.accountName) setWho(answer.accountName);
      } catch (value) {
        // A failed check says nothing about the attempt, so the code stays on
        // screen and the next tick tries again.
        const failure = value instanceof CommandError ? value : null;
        setProblem(failure?.message ?? "The code could not be checked. Trying again.");
      }
    };

    const timer = window.setInterval(() => void tick(), POLL_MS);
    return () => {
      stop = true;
      window.clearInterval(timer);
    };
  }, [stage, finish]);

  // Leaving the drawer with an attempt in flight frees it, so the person does
  // not end up with an approval waiting on an account they abandoned.
  useEffect(
    () => () => {
      const current = live.current;
      if (!current || finished.current) return;
      void api.quickLoginCancel(current.code, current.key).catch(() => undefined);
    },
    [],
  );

  if (stage === "idle" || stage === "starting") {
    return (
      <div className="px-3.5 py-3">
        <button
          type="button"
          onClick={() => void start()}
          disabled={stage === "starting"}
          className="ui h-8 w-full rounded-control bg-gold text-[12px] font-semibold text-page hover:bg-[#f0cd7c] disabled:opacity-50"
        >
          {stage === "starting" ? "Asking Roblox for a code" : "Sign in with a code"}
        </button>
        <p className="mt-2 text-[11px] leading-[1.55] text-muted">
          Roblox shows a six character code here. Enter it on the Roblox website or scan the
          square with the app on your phone, and this machine is signed in without a password
          or a pasted cookie.
        </p>
        {problem ? <p className="mt-1.5 text-[11px] leading-[1.55] text-danger">{problem}</p> : null}
      </div>
    );
  }

  if (stage === "done") {
    return (
      <div className="px-3.5 py-3">
        <p className="text-[13px] text-body">
          Signed in{who ? ` as ${who}` : ""}. The session is sealed on this machine.
        </p>
        <button
          type="button"
          onClick={() => {
            setStage("idle");
            setWho(null);
          }}
          className="ui mt-2 h-7 rounded-control border border-white/10 px-2.5 text-[12px] text-muted hover:text-body"
        >
          Add another
        </button>
      </div>
    );
  }

  if (stage === "cancelled" || stage === "expired") {
    return (
      <div className="px-3.5 py-3">
        <p className="text-[13px] text-body">
          {stage === "cancelled" ? "The sign-in was cancelled." : "That code expired."}
        </p>
        <p className="mt-1 text-[11px] leading-[1.55] text-muted">
          {stage === "cancelled"
            ? "Nothing was stored. A new code is one press away."
            : "A code stays valid for two minutes. Start again to get a fresh one."}
        </p>
        <button
          type="button"
          onClick={() => void start()}
          className="ui mt-2 h-7 rounded-control border border-white/10 px-2.5 text-[12px] text-muted hover:text-body"
        >
          Start again
        </button>
      </div>
    );
  }

  // waiting or finishing
  const clock = `${Math.floor(remaining / 60)}:${String(remaining % 60).padStart(2, "0")}`;
  const busy = stage === "finishing";

  return (
    <div className="px-3.5 py-3.5">
      <div className="flex gap-3.5">
        {attempt?.qr ? (
          <img
            src={attempt.qr}
            alt="Sign-in code as a square for the Roblox app"
            width={132}
            height={132}
            className="shrink-0 rounded-[4px] bg-white p-4"
            style={{ imageRendering: "pixelated" }}
          />
        ) : null}

        <div className="min-w-0 flex-1">
          <div className="text-[11px] text-muted">Enter this on roblox.com</div>
          <div
            className="num mt-1 text-[26px] leading-none font-semibold tracking-[0.16em] text-body select-all"
            aria-live="polite"
          >
            {attempt?.code}
          </div>

          <div className="mt-2.5 text-[11px] leading-[1.55] text-muted">
            {busy ? (
              "Approved. Finishing the sign-in."
            ) : who ? (
              `${who} approved this code. Finishing.`
            ) : (
              <>
                Waiting. It expires in <span className="num text-body">{clock}</span>.
              </>
            )}
          </div>

          <div className="mt-2.5 flex items-center gap-1.5">
            <button
              type="button"
              onClick={() => void api.openSite("https://www.roblox.com/crossdevicelogin/ConfirmCode")}
              className="ui h-7 rounded-control border border-white/10 px-2.5 text-[12px] text-muted hover:text-body"
            >
              Open the page
            </button>
            <button
              type="button"
              onClick={cancel}
              className="ui h-7 rounded-control px-2 text-[12px] text-muted hover:bg-white/5 hover:text-body"
            >
              Cancel
            </button>
          </div>
        </div>
      </div>

      {problem ? (
        <p className={cn("mt-2.5 text-[11px] leading-[1.55] text-danger")}>
          {problem}
          {problem.includes("could not be checked") ? " The code above still works." : null}
        </p>
      ) : null}
    </div>
  );
}
