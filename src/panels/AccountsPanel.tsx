import { Check, KeyRound, ShieldCheck, Star, Trash2 } from "lucide-react";
import { useEffect, useRef, useState } from "react";

import { Badge, Row, Section } from "../components/Panel";
import { QuickLogin } from "../components/QuickLogin";
import { cn } from "../lib/cn";
import { day } from "../lib/format";
import type { AccountView } from "../lib/types";
import { useAccounts } from "../store/accounts";
import { useShell } from "../store/shell";

/**
 * The accounts this machine can launch as.
 *
 * The one rule that shapes this screen: a session cookie is never shown again
 * once it is added. There is no "reveal" button, no copy button and no field
 * that echoes it, because every one of those would be a way to leak a session
 * that lives in the vault on purpose.
 */
export function AccountsBody() {
  const load = useAccounts((state) => state.load);

  useEffect(() => {
    void load();
  }, [load]);

  return (
    <>
      <AddAccount />
      <Saved />
    </>
  );
}

function AddAccount() {
  const add = useAccounts((state) => state.add);
  const adding = useAccounts((state) => state.adding);
  const [value, setValue] = useState("");
  const field = useRef<HTMLInputElement>(null);

  const submit = async () => {
    const trimmed = value.trim();
    if (!trimmed || adding) return;
    const stored = await add(trimmed);
    if (stored) setValue("");
    field.current?.focus();
  };

  return (
    <Section title="Add an account">
      <QuickLogin />

      <div className="border-t border-white/6 px-3.5 py-3">
        <label htmlFor="cookie" className="text-[13px] text-body">
          Or paste your <span className="num">.ROBLOSECURITY</span> value
        </label>
        <p className="mt-1 text-[11px] leading-[1.55] text-muted">
          The fallback for when you cannot reach a device you are signed in on. It is checked
          against Roblox, sealed with Windows, and never shown again. It never leaves this
          machine.
        </p>

        <div className="mt-2.5 flex items-center gap-1.5">
          <input
            id="cookie"
            ref={field}
            type="password"
            value={value}
            onChange={(event) => setValue(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") void submit();
            }}
            placeholder="Paste here"
            autoComplete="off"
            spellCheck={false}
            className="ui h-8 min-w-0 flex-1 rounded-control border border-white/10 bg-raised px-2.5 font-mono text-[12px] text-body placeholder:font-sans placeholder:text-muted focus:border-gold/60 focus:outline-none"
          />
          <button
            type="button"
            onClick={() => void submit()}
            disabled={!value.trim() || adding}
            className="ui h-8 shrink-0 rounded-control bg-gold px-3 text-[12px] font-semibold text-page hover:bg-[#f0cd7c] disabled:opacity-40"
          >
            {adding ? "Checking" : "Add"}
          </button>
        </div>
      </div>
    </Section>
  );
}

function Saved() {
  const accounts = useAccounts((state) => state.accounts);
  const loading = useAccounts((state) => state.loading);
  const checking = useAccounts((state) => state.checking);
  const check = useAccounts((state) => state.check);

  const list = accounts ?? [];

  return (
    <Section
      title="Saved on this machine"
      aside={
        list.length > 0 ? (
          <button
            type="button"
            onClick={() => void check()}
            disabled={checking}
            className="ui -mt-1 flex h-6 items-center gap-1.5 rounded-control px-2 text-[11px] text-muted hover:bg-white/5 hover:text-body disabled:opacity-50"
          >
            <ShieldCheck size={12} strokeWidth={1.8} />
            {checking ? "Checking" : "Check all"}
          </button>
        ) : null
      }
    >
      {loading && accounts === null ? (
        <Row label="Reading the vault" />
      ) : list.length === 0 ? (
        <Row
          label="No account yet"
          hint="Add one above. Anything that needs a session waits until there is one."
        />
      ) : (
        list.map((account) => <AccountRow key={account.userId} account={account} />)
      )}

      {list.length > 0 ? (
        <div className="px-3.5 py-3 text-[11px] leading-[1.55] text-muted">
          Forgetting an account removes it from this machine. The session stays open on Roblox
          until it expires or you sign out everywhere, so sign out there as well if you need it
          closed.
        </div>
      ) : null}
    </Section>
  );
}

function AccountRow({ account }: { account: AccountView }) {
  const makeDefault = useAccounts((state) => state.makeDefault);
  const forget = useAccounts((state) => state.forget);
  const [confirming, setConfirming] = useState(false);

  const tone =
    account.health === "ok" ? "success" : account.health === "expired" ? "danger" : "muted";

  return (
    <div className="border-b border-white/6 last:border-b-0">
      <div className="flex min-h-[46px] items-center justify-between gap-4 px-3.5 py-2.5">
        <div className="min-w-0">
          <div className="flex items-center gap-1.5">
            <span className="truncate text-[13px] text-body">{account.display}</span>
            {account.isDefault ? <Badge tone="gold">default</Badge> : null}
            {account.isLastUsed && !account.isDefault ? <Badge>last used</Badge> : null}
          </div>
          <div className="mt-0.5 text-[11px] leading-[1.5] text-muted">
            <span className="num">@{account.name}</span>
            <span aria-hidden> · </span>
            <span className="num">{account.userId}</span>
            <span aria-hidden> · </span>
            added {day(account.added)}
          </div>
        </div>

        <div className="flex shrink-0 items-center gap-1">
          <Badge tone={tone}>
            {account.health === "ok" ? "live" : account.health === "expired" ? "expired" : "unchecked"}
          </Badge>

          {account.isDefault ? (
            <span className="flex h-7 w-7 items-center justify-center text-gold" aria-hidden>
              <Star size={14} strokeWidth={1.7} />
            </span>
          ) : (
            <button
              type="button"
              onClick={() => void makeDefault(account.userId)}
              aria-label={`Make ${account.display} the default`}
              title="Make default"
              className="ui flex h-7 w-7 items-center justify-center rounded-control text-muted hover:bg-white/6 hover:text-body"
            >
              <Star size={14} strokeWidth={1.7} />
            </button>
          )}

          <button
            type="button"
            onClick={() => setConfirming(true)}
            aria-label={`Forget ${account.display}`}
            title="Forget on this machine"
            className="ui flex h-7 w-7 items-center justify-center rounded-control text-muted hover:bg-danger-fill/15 hover:text-danger"
          >
            <Trash2 size={14} strokeWidth={1.7} />
          </button>
        </div>
      </div>

      {account.health === "expired" ? (
        <div className="border-t border-white/6 px-3.5 py-2.5">
          <p className="flex items-start gap-2 text-[11px] leading-[1.55] text-muted">
            <KeyRound size={12} strokeWidth={1.8} className="mt-0.5 shrink-0 text-danger" />
            This session no longer works. Paste a fresh cookie above, or sign in on the website
            and copy the new value.
          </p>
        </div>
      ) : null}

      {confirming ? (
        <div className="flex items-center justify-between gap-3 border-t border-white/6 bg-danger-fill/8 px-3.5 py-2.5">
          <p className="text-[11px] leading-[1.55] text-muted">
            Forgetting {account.display} deletes the sealed session from this machine. It is a
            permanent delete, and the session stays open on Roblox.
          </p>
          <div className="flex shrink-0 items-center gap-1.5">
            <button
              type="button"
              onClick={() => setConfirming(false)}
              className="ui h-7 rounded-control border border-white/10 px-2.5 text-[12px] text-muted hover:text-body"
            >
              Keep
            </button>
            <button
              type="button"
              onClick={() => {
                setConfirming(false);
                void forget(account.userId);
              }}
              className={cn(
                "ui h-7 rounded-control bg-danger-fill px-2.5 text-[12px] font-semibold text-white",
                "hover:bg-[#b8373b]",
              )}
            >
              Forget
            </button>
          </div>
        </div>
      ) : null}
    </div>
  );
}

/** The strip Library shows, so an account is visible without opening a drawer. */
export function useAccountStrip(): AccountView[] {
  const accounts = useAccounts((state) => state.accounts);
  const load = useAccounts((state) => state.load);

  useEffect(() => {
    if (accounts === null) void load();
  }, [accounts, load]);

  return accounts ?? [];
}

export function AccountStrip() {
  const accounts = useAccountStrip();
  const go = useShell((state) => state.go);

  if (accounts.length === 0) return null;

  return (
    <div className="flex flex-wrap items-center gap-1.5">
      {accounts.map((account) => (
        <span
          key={account.userId}
          className="flex items-center gap-1.5 rounded-control border border-white/8 bg-panel px-2 py-1 text-[12px]"
        >
          <span
            aria-hidden
            className={cn(
              "h-1.5 w-1.5 rounded-full",
              account.health === "ok"
                ? "bg-success"
                : account.health === "expired"
                  ? "bg-danger-fill"
                  : "bg-white/25",
            )}
          />
          <span className="text-body">{account.display}</span>
          {account.isDefault ? (
            <Check size={11} strokeWidth={2} className="text-gold" aria-label="default" />
          ) : null}
        </span>
      ))}
      <button
        type="button"
        onClick={() => go("accounts")}
        className="ui rounded-control px-2 py-1 text-[12px] text-muted hover:bg-white/5 hover:text-body"
      >
        Manage
      </button>
    </div>
  );
}
