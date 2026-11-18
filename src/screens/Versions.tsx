import { PageFrame } from "../components/PageFrame";
import { VersionsBody } from "../panels/VersionsPanel";

export function VersionsScreen() {
  return (
    <PageFrame
      title="Versions"
      subtitle="Which clients are on this machine, which one Roblox is serving, and how to get a missing one without guessing at a hash."
    >
      <VersionsBody />
    </PageFrame>
  );
}
