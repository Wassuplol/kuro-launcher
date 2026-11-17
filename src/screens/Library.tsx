import { EmptyState } from "../components/EmptyState";

export function LibraryScreen() {
  return (
    <div className="flex h-full items-center justify-center px-8">
      <EmptyState
        title="Your library is empty"
        body="Accounts, favourites and history collect here. Connect an account and this page starts filling."
      />
    </div>
  );
}
