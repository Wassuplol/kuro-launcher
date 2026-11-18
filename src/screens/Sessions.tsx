import { PageFrame } from "../components/PageFrame";
import { SessionsBody } from "../panels/SessionsPanel";
import { api } from "../lib/ipc";

export function SessionsScreen() {
  return (
    <PageFrame
      title="Sessions"
      subtitle="Every launch this machine still has a log for, read from the client's own file: where it went, how long it lasted, and what actually happened when it did not work."
      aside={
        <button
          type="button"
          onClick={() => void api.stripOpen()}
          className="ui h-7 rounded-control border border-white/10 px-2.5 text-[12px] text-muted hover:border-white/20 hover:text-body"
        >
          In-game strip
        </button>
      }
    >
      <SessionsBody />
    </PageFrame>
  );
}
