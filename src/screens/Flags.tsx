import { PageFrame } from "../components/PageFrame";
import { FlagsBody } from "../panels/FlagsPanel";

export function FlagsScreen() {
  return (
    <PageFrame
      title="Flags"
      subtitle="Roblox's own flags, in named profiles, resolved per flag across account, game and global, and written where the client reads them."
    >
      <FlagsBody />
    </PageFrame>
  );
}
