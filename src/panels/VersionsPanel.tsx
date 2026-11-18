import { Check, HardDriveDownload, RefreshCw, ShieldCheck, Trash2, Wrench } from "lucide-react";
import { useEffect, useRef, useState, type ReactNode } from "react";

import { Badge, Row, Section } from "../components/Panel";
import { cn } from "../lib/cn";
import { bytes, clock, day } from "../lib/format";
import type {
  Channel,
  InstallMethod,
  InstalledClient,
  LiveVersion,
  SupplySnapshot,
  VerifyReport,
} from "../lib/types";
import { useShell } from "../store/shell";
import { useVersions } from "../store/versions";

const METHOD_LABEL: Record<InstallMethod, { title: string; detail: string }> = {
  bootstrap: {
    title: "Roblox's installer",
    detail: "Roblox's own program does the work. It is the default because it is the path Roblox supports.",
  },
  direct: {
    title: "Direct from Roblox's CDN",
    detail: "Every package is checked against the hash Roblox publishes, then moved into place in one step.",
  },
};

export function VersionsBody() {
  // The versions store holds only what was read from Roblox and from the disk;
  // this page owns no state of its own.
  const load = useVersions((state) => state.load);
  const refresh = useVersions((state) => state.refresh);
  const livePlayer = useVersions((state) => state.snapshot?.livePlayer ?? null);

  // One read from disk per open, and one question to Roblox if this session has
  // not asked yet. A failed ask is not retried on a loop: the Check button is
  // the retry, and it says when it last ran.
  const asked = useRef(false);

  useEffect(() => {
    void load();
  }, [load]);

  useEffect(() => {
    if (livePlayer || asked.current) return;
    asked.current = true;
    void refresh();
  }, [livePlayer, refresh]);

  return (
    <>
      <Serving />
      <Install />
      <OnThisMachine />
      <Space />
      <Studio />
    </>
  );
}

/** What Roblox is serving, and the channels this app can actually read. */
function Serving() {
  const snapshot = useVersions((state) => state.snapshot);
  const refreshing = useVersions((state) => state.refreshing);
  const refresh = useVersions((state) => state.refresh);

  const live = snapshot?.livePlayer ?? null;
  const channels = snapshot?.channels ?? [];

  return (
    <Section
      title="Roblox is serving"
      aside={
        <button
          type="button"
          onClick={() => void refresh()}
          disabled={refreshing}
          className="ui -mt-1 flex h-6 items-center gap-1.5 rounded-control px-2 text-[11px] text-muted hover:bg-white/5 hover:text-body disabled:opacity-50"
        >
          <RefreshCw size={12} strokeWidth={1.8} className={cn(refreshing && "animate-spin")} />
          {refreshing ? "Asking" : "Check"}
        </button>
      }
    >
      {channels.map((channel) => (
        <ChannelRow
          key={channel.id}
          channel={channel}
          live={channel.id === live?.channel ? live : null}
        />
      ))}
    </Section>
  );
}

/**
 * One channel. The live version hangs off the channel it came from, so the
 * answer and its source are the same row instead of the same word twice.
 */
function ChannelRow({ channel, live }: { channel: Channel; live: LiveVersion | null }) {
  const hint =
    live !== null
      ? `Asked at ${clock(live.checkedAt)}.`
      : (channel.note ?? "Readable without a signed-in session.");

  return (
    <Row label={<span className="num">{channel.label}</span>} hint={hint}>
      <div className="flex shrink-0 items-center gap-2">
        {live ? <span className="num text-[13px] text-body">{live.version}</span> : null}
        <Badge tone={channel.available ? "success" : "muted"}>
          {channel.available ? "open" : "closed"}
        </Badge>
      </div>
    </Row>
  );
}

/** Install, only when there is something to install. */
function Install() {
  const snapshot = useVersions((state) => state.snapshot);
  const installing = useVersions((state) => state.installing);
  const progress = useVersions((state) => state.progress);
  const install = useVersions((state) => state.install);
  const cancel = useVersions((state) => state.cancel);

  const [method, setMethod] = useState<InstallMethod>("bootstrap");

  const live = snapshot?.livePlayer;

  if (installing) {
    const fraction =
      progress && progress.total > 0 ? Math.min(1, progress.done / progress.total) : null;

    return (
      <Section title="Installing">
        <div className="px-3.5 py-3">
          <div className="flex items-baseline justify-between gap-3">
            <span className="text-[13px] text-body">{progress?.label ?? "Starting"}</span>
            <span className="num text-[11px] text-muted">
              {progress && progress.total > 0
                ? `${bytes(progress.done)} of ${bytes(progress.total)}`
                : ""}
            </span>
          </div>

          <div className="mt-2.5 h-[3px] overflow-hidden rounded-full bg-white/8">
            <div
              className="h-full rounded-full bg-gold transition-[width] duration-200 ease-out"
              style={{ width: fraction === null ? "100%" : `${fraction * 100}%` }}
              aria-hidden
            />
          </div>

          <div className="mt-3 flex items-center justify-between gap-3">
            <p className="text-[11px] leading-[1.5] text-muted">
              The window stays usable while this runs. Nothing is written into the versions folder
              until the download has been checked.
            </p>
            <button
              type="button"
              onClick={() => void cancel()}
              className="ui h-7 shrink-0 rounded-control border border-white/10 px-2.5 text-[12px] text-muted hover:border-white/20 hover:text-body"
            >
              Stop
            </button>
          </div>
        </div>
      </Section>
    );
  }

  if (!live) return null;

  // Narrowed above, so the upload folder can be compared without a guard.
  const alreadyHere = snapshot?.installed.some((client) => client.folder === live.upload);

  if (alreadyHere) {
    return (
      <Section title="Install">
        <Row
          label="The live version is installed"
          hint={`${live.version} is already in the versions folder.`}
        >
          <Check size={15} strokeWidth={1.8} className="text-success" />
        </Row>
      </Section>
    );
  }

  return (
    <Section title="Install">
      <Row
        label={<span className="num">{live.version}</span>}
        hint="Roblox is serving this and it is not on this machine yet."
      >
        <button
          type="button"
          onClick={() => void install("player", live.upload, method)}
          className="ui flex h-7 shrink-0 items-center gap-1.5 rounded-control bg-gold px-2.5 text-[12px] font-semibold text-page hover:bg-[#f0cd7c]"
        >
          <HardDriveDownload size={13} strokeWidth={1.9} />
          Install
        </button>
      </Row>

      <div className="border-b border-white/6 px-3.5 py-3">
        <div className="label">How</div>
        <div className="mt-2 space-y-1.5" role="radiogroup" aria-label="How to install">
          {(Object.keys(METHOD_LABEL) as InstallMethod[]).map((key) => {
            const on = method === key;
            return (
              <button
                key={key}
                type="button"
                role="radio"
                aria-checked={on}
                onClick={() => setMethod(key)}
                className={cn(
                  "ui flex w-full items-start gap-2.5 rounded-control border px-2.5 py-2 text-left",
                  on
                    ? "border-gold/45 bg-gold/8"
                    : "border-white/8 hover:border-white/16 hover:bg-white/4",
                )}
              >
                <span
                  aria-hidden
                  className={cn(
                    "mt-[3px] flex h-3 w-3 shrink-0 items-center justify-center rounded-full border",
                    on ? "border-gold" : "border-white/25",
                  )}
                >
                  {on ? <span className="h-1.5 w-1.5 rounded-full bg-gold" /> : null}
                </span>
                <span className="min-w-0">
                  <span className={cn("block text-[12px]", on ? "text-gold" : "text-body")}>
                    {METHOD_LABEL[key].title}
                  </span>
                  <span className="mt-0.5 block text-[11px] leading-[1.5] text-muted">
                    {METHOD_LABEL[key].detail}
                  </span>
                </span>
              </button>
            );
          })}
        </div>
        <p className="mt-2.5 text-[11px] leading-[1.5] text-muted">
          If the direct install fails at any step, Roblox's installer is started automatically, so
          the attempt is never simply lost.
        </p>
      </div>
    </Section>
  );
}

/** Every client folder on the machine, with the actions that apply to it. */
function OnThisMachine() {
  const snapshot = useVersions((state) => state.snapshot);
  const loading = useVersions((state) => state.loading);

  const players = (snapshot?.installed ?? []).filter((client) => client.kind !== "studio");

  return (
    <Section
      title="On this machine"
      aside={
        snapshot ? (
          <span className="num text-[11px] text-muted">
            {players.length} {players.length === 1 ? "version" : "versions"}
          </span>
        ) : null
      }
    >
      {loading && !snapshot ? (
        <Row label="Reading the versions folder" />
      ) : players.length === 0 ? (
        <Row
          label="No Roblox client found"
          hint={
            snapshot?.exists
              ? "The versions folder exists but holds no client."
              : "There is no Roblox versions folder on this machine yet."
          }
        />
      ) : (
        players.map((client) => <InstalledRow key={client.folder} client={client} />)
      )}
    </Section>
  );
}

function InstalledRow({ client }: { client: InstalledClient }) {
  const advanced = useShell((state) => state.settings?.advanced ?? false);
  const pin = useVersions((state) => state.pin);
  const check = useVersions((state) => state.check);
  const repair = useVersions((state) => state.repair);
  const remove = useVersions((state) => state.remove);
  const report = useVersions((state) => state.checks[client.folder]);

  const [confirming, setConfirming] = useState(false);
  const busy = useVersions((state) => state.installing?.upload === client.folder);

  const removable = !client.live && !client.pinned && !client.holder;

  return (
    <div className="border-b border-white/6 last:border-b-0">
      <div className="flex min-h-[46px] items-center justify-between gap-4 px-3.5 py-2.5">
        <div className="min-w-0">
          <div className="flex items-center gap-1.5">
            <span className="num text-[13px] text-body">
              {client.version ?? "no version resource"}
            </span>
            {client.live ? <Badge tone="gold">live</Badge> : null}
            {client.pinned ? <Badge tone="success">pinned</Badge> : null}
            {client.holder ? <Badge tone="danger">in use</Badge> : null}
            {!client.real ? <Badge tone="danger">not a client</Badge> : null}
          </div>
          <div className="mt-0.5 text-[11px] leading-[1.5] text-muted">
            <span className="num">{bytes(client.bytes)}</span>
            <span aria-hidden> · </span>
            <span className="num">{client.files} files</span>
            {client.modified ? (
              <>
                <span aria-hidden> · </span>
                {day(client.modified)}
              </>
            ) : null}
            {advanced ? (
              <>
                <span aria-hidden> · </span>
                {client.root}
                <span aria-hidden> · </span>
                <span className="num">{client.folder}</span>
              </>
            ) : null}
          </div>
        </div>

        <div className="flex shrink-0 items-center gap-1">
          <IconAction
            label={client.pinned ? "Unpin this version" : "Pin this version"}
            onClick={() => void pin(client.pinned ? null : client.folder)}
          >
            <ShieldCheck
              size={14}
              strokeWidth={1.7}
              className={client.pinned ? "text-success" : undefined}
            />
          </IconAction>

          <IconAction
            label="Check every file against its hash"
            onClick={() => void check(client.folder)}
          >
            <Wrench size={14} strokeWidth={1.7} />
          </IconAction>

          {removable ? (
            <IconAction
              label={`Remove ${client.folder}`}
              danger
              onClick={() => setConfirming(true)}
            >
              <Trash2 size={14} strokeWidth={1.7} />
            </IconAction>
          ) : null}
        </div>
      </div>

      {confirming ? (
        <div className="flex items-center justify-between gap-3 border-t border-white/6 bg-danger-fill/8 px-3.5 py-2.5">
          <p className="text-[11px] leading-[1.5] text-muted">
            Removing {client.version ?? client.folder} deletes {bytes(client.bytes)} from this
            machine. It is a permanent delete, not a trip to the Recycle Bin. Roblox downloads it
            again if it needs it.
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
                void remove(client.folder);
              }}
              className="ui h-7 rounded-control bg-danger-fill px-2.5 text-[12px] font-semibold text-white hover:bg-[#b8373b]"
            >
              Remove
            </button>
          </div>
        </div>
      ) : null}

      {report ? <CheckResult report={report} onRepair={() => void repair(client.folder)} busy={busy} /> : null}
    </div>
  );
}

function CheckResult({
  report,
  onRepair,
  busy,
}: {
  report: VerifyReport;
  onRepair: () => void;
  busy: boolean;
}) {
  const clean = report.tracked && report.mismatched.length === 0 && report.missing.length === 0;
  const damaged = report.mismatched.length + report.missing.length;

  if (!report.tracked) {
    return (
      <div className="border-t border-white/6 px-3.5 py-2.5">
        <p className="text-[11px] leading-[1.5] text-muted">
          Kuro did not install this folder, so there are no hashes to check it against. Roblox's
          own copy is still covered by its signature.
        </p>
      </div>
    );
  }

  return (
    <div className="border-t border-white/6 px-3.5 py-2.5">
      <div className="flex items-center justify-between gap-3">
        <p className="text-[11px] leading-[1.5] text-muted">
          {clean ? (
            <>
              All <span className="num">{report.checked}</span> files match the hashes written when
              Kuro installed this.
            </>
          ) : (
            <>
              <span className="num">{damaged}</span> of{" "}
              <span className="num">{report.checked}</span> files no longer match. Repair fetches{" "}
              {report.packages.length === 1 ? "one package" : `${report.packages.length} packages`}{" "}
              again and unpacks them over this folder.
            </>
          )}
        </p>
        {clean ? (
          <Check size={14} strokeWidth={1.9} className="shrink-0 text-success" />
        ) : (
          <button
            type="button"
            onClick={onRepair}
            disabled={busy}
            className="ui h-7 shrink-0 rounded-control border border-gold/40 px-2.5 text-[12px] text-gold hover:bg-gold/10 disabled:opacity-50"
          >
            Repair
          </button>
        )}
      </div>
    </div>
  );
}

/** Disk use, and the prune, with its reasoning shown before anything is deleted. */
function Space() {
  const snapshot = useVersions((state) => state.snapshot);
  const plan = useVersions((state) => state.plan);
  const reviewPrune = useVersions((state) => state.reviewPrune);
  const prune = useVersions((state) => state.prune);
  const dismiss = useVersions((state) => state.dismissPlan);

  if (!snapshot) return null;

  return (
    <Section title="Space">
      <Row
        label={<span className="num">{bytes(snapshot.totalBytes)}</span>}
        hint={`${snapshot.installed.length} ${
          snapshot.installed.length === 1 ? "folder" : "folders"
        } across ${usedRoots(snapshot)} versions ${
          usedRoots(snapshot) === 1 ? "folder" : "folders"
        }. New clients go to ${rootName(snapshot)}.`}
      >
        {plan ? null : (
          <button
            type="button"
            onClick={() => void reviewPrune()}
            className="ui h-7 shrink-0 rounded-control border border-white/10 px-2.5 text-[12px] text-body hover:border-white/20 hover:bg-white/5"
          >
            Review old versions
          </button>
        )}
      </Row>

      {plan ? (
        <div className="border-t border-white/6 px-3.5 py-3">
          {plan.remove.length === 0 ? (
            <p className="text-[12px] leading-[1.6] text-muted">
              Nothing to remove. Every folder is either pinned, served by Roblox, the newest copy,
              or in use.
            </p>
          ) : (
            <p className="text-[12px] leading-[1.6] text-body">
              Removing <span className="num">{plan.remove.length}</span>{" "}
              {plan.remove.length === 1 ? "version" : "versions"} frees{" "}
              <span className="num">{bytes(plan.bytes)}</span>. This deletes them permanently.
            </p>
          )}

          <ul className="mt-2 space-y-1">
            {plan.keep.map((entry) => (
              <li key={entry.folder} className="text-[11px] leading-[1.5] text-muted">
                <span className="num">{entry.folder}</span>
                <span aria-hidden> · </span>
                {entry.reason}
              </li>
            ))}
          </ul>

          <div className="mt-3 flex items-center gap-1.5">
            <button
              type="button"
              onClick={dismiss}
              className="ui h-7 rounded-control border border-white/10 px-2.5 text-[12px] text-muted hover:text-body"
            >
              Cancel
            </button>
            {plan.remove.length > 0 ? (
              <button
                type="button"
                onClick={() => void prune()}
                className="ui h-7 rounded-control bg-danger-fill px-2.5 text-[12px] font-semibold text-white hover:bg-[#b8373b]"
              >
                Remove {plan.remove.length}
              </button>
            ) : null}
          </div>
        </div>
      ) : null}
    </Section>
  );
}

function Studio() {
  const snapshot = useVersions((state) => state.snapshot);
  const installing = useVersions((state) => state.installing);
  const install = useVersions((state) => state.install);

  const studio = (snapshot?.installed ?? []).filter((client) => client.kind === "studio");
  const live = snapshot?.liveStudio ?? null;
  const busy = installing?.kind === "studio";

  return (
    <Section title="Studio">
      {studio.length > 0 ? (
        studio.map((client) => (
          <Row
            key={client.folder}
            label={<span className="num">{client.version ?? client.folder}</span>}
            hint={`${bytes(client.bytes)} installed.`}
          >
            <Badge tone="success">installed</Badge>
          </Row>
        ))
      ) : (
        <Row
          label="Roblox Studio is not installed"
          hint={
            live
              ? `${live.version} is the version Roblox is serving.`
              : "Studio is detected by the same scan that finds the player."
          }
        >
          {live ? (
            <button
              type="button"
              onClick={() => void install("studio", live.upload, "bootstrap")}
              disabled={busy}
              className="ui h-7 shrink-0 rounded-control border border-white/10 px-2.5 text-[12px] text-body hover:border-white/20 hover:bg-white/5 disabled:opacity-50"
            >
              Install
            </button>
          ) : null}
        </Row>
      )}
    </Section>
  );
}

/** How many roots actually hold something, so a missing folder is not counted. */
function usedRoots(snapshot: SupplySnapshot): number {
  return snapshot.roots.filter((root) => root.exists).length;
}

/** The install root in words, because a raw path tells a person nothing. */
function rootName(snapshot: SupplySnapshot): string {
  const match = snapshot.roots.find((root) => root.path === snapshot.versionsRoot);
  if (!match) return snapshot.versionsRoot;
  return match.perUser ? "your own folder" : "the all-users folder";
}

function IconAction({
  label,
  onClick,
  danger,
  children,
}: {
  label: string;
  onClick: () => void;
  danger?: boolean;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-label={label}
      title={label}
      className={cn(
        "ui flex h-7 w-7 items-center justify-center rounded-control text-muted",
        danger ? "hover:bg-danger-fill/15 hover:text-danger" : "hover:bg-white/6 hover:text-body",
      )}
    >
      {children}
    </button>
  );
}
