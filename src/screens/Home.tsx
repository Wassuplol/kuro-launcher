import { EmptyState } from "../components/EmptyState";
import { greeting, listNames } from "../lib/format";
import { useShell } from "../store/shell";

export function HomeScreen() {
  const settings = useShell((state) => state.settings);
  const name = settings?.identity.name.trim() ?? "";
  const charts = listNames(settings?.identity.genres ?? []);

  return (
    <div className="h-full overflow-y-auto">
      <div className="px-8 pt-8 pb-7">
        <h1 className="text-[22px] font-semibold tracking-[-0.015em] text-body">
          {greeting()}
          {name ? `, ${name}` : ""}.
        </h1>
        <p className="mt-1.5 text-[13px] text-muted">
          One place to launch Roblox, without touching the client.
        </p>
      </div>

      <section className="px-8 pb-10">
        <h2 className="text-[13px] font-semibold text-body">Popular</h2>
        <div className="mt-3 rounded-card border border-white/8 bg-panel p-6">
          <EmptyState
            level={2}
            spread
            title="The shelf is empty"
            body={
              charts
                ? `Popular games in ${charts} load from Roblox. This version cannot reach it yet, so nothing is listed.`
                : "Popular games load from Roblox. This version cannot reach it yet, so nothing is listed."
            }
          />
        </div>
      </section>
    </div>
  );
}
