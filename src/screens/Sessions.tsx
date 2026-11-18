import { PageFrame } from "../components/PageFrame";
import { SessionsBody } from "../panels/SessionsPanel";

export function SessionsScreen() {
  return (
    <PageFrame
      title="Sessions"
      subtitle="Every launch this machine still has a log for, read from the client's own file: where it went, how long it lasted, and what actually happened when it did not work."
    >
      <SessionsBody />
    </PageFrame>
  );
}
