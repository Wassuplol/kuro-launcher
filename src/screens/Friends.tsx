import { EmptyState } from "../components/EmptyState";

export function FriendsScreen() {
  return (
    <div className="flex h-full items-center justify-center px-8">
      <EmptyState
        mirror
        title="No friends list yet"
        body="Your friends, their presence and a Join button live here. The list needs a connected account."
      />
    </div>
  );
}
