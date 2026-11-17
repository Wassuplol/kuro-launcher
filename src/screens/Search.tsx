import { EmptyState } from "../components/EmptyState";

export function SearchScreen() {
  return (
    <div className="flex h-full items-center justify-center px-8">
      <EmptyState
        art={false}
        title="Nothing to search yet"
        body="Search reads games and people from Roblox. This version has no connection to it, so there is nothing to query."
        hint="Ctrl K runs the commands that do work today."
      />
    </div>
  );
}
