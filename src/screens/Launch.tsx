import { PageFrame } from "../components/PageFrame";
import { LaunchBody } from "../panels/LaunchPanel";

export function LaunchScreen() {
  return (
    <PageFrame
      title="Launch"
      subtitle="Paste a game link, a private server invite or a place id. It reads the destination before anything starts, so the mode is visible before the game is."
    >
      <LaunchBody />
    </PageFrame>
  );
}
