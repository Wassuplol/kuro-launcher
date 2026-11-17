import { EmptyState } from "../components/EmptyState";
import { AccountStrip } from "../panels/AccountsPanel";

export function LibraryScreen() {
  return (
    <div className="flex h-full flex-col">
      <div className="shrink-0 px-8 pt-8">
        <AccountStrip />
      </div>
      <div className="flex min-h-0 flex-1 items-center justify-center px-8">
        <EmptyState
          title="Your library is empty"
          body="Favourites and history collect here. Connect an account and this page starts filling."
        />
      </div>
    </div>
  );
}
