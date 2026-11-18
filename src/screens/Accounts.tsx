import { PageFrame } from "../components/PageFrame";
import { AccountsBody } from "../panels/AccountsPanel";

export function AccountsScreen() {
  return (
    <PageFrame
      title="Accounts"
      subtitle="The sessions this machine can launch as. A session is verified against Roblox, sealed with Windows, and never shown again."
    >
      <AccountsBody />
    </PageFrame>
  );
}
