import {
  ButtonItem,
  PanelSection,
  PanelSectionRow,
  staticClasses,
} from "@decky/ui";
import { callable, definePlugin, toaster } from "@decky/api";
import { useEffect, useState } from "react";
import { FaArrowsRotate } from "react-icons/fa6";

type NamedItem = { id: string; name: string };
type Summary = {
  ready: boolean;
  error?: string;
  device?: NamedItem;
  peers?: NamedItem[];
  folders?: NamedItem[];
};
type Plan = {
  folder_id: string;
  peer_id: string;
  local_hash: string;
  remote_hash: string;
  base_hash?: string;
  decision: "in_sync" | "push" | "pull" | "conflict";
  reason: string;
};

const getSummary = callable<[], Summary>("summary");
const getPlan = callable<[peerId: string, folderId: string], Plan>("plan");
const runSync = callable<
  [peerId: string, folderId: string, action: string, acceptConflict: boolean],
  { root_hash: string }
>("sync");

function shortHash(value?: string): string {
  return value ? value.slice(0, 12) : "none";
}

function Content() {
  const [summary, setSummary] = useState<Summary>({ ready: false });
  const [plans, setPlans] = useState<Record<string, Plan>>({});
  const [busy, setBusy] = useState<string>();
  const [pending, setPending] = useState<{
    folderId: string;
    action: "push" | "pull";
  }>();

  const refresh = async () => {
    const next = await getSummary();
    setSummary(next);
    if (!next.ready || !next.peers?.[0]) return;
    const peer = next.peers[0];
    const entries = await Promise.all(
      (next.folders ?? []).map(async (folder) => [
        folder.id,
        await getPlan(peer.id, folder.id),
      ] as const),
    );
    setPlans(Object.fromEntries(entries));
  };

  useEffect(() => {
    void refresh();
  }, []);

  const sync = async (
    folderId: string,
    action: "auto" | "push" | "pull",
    confirmed = false,
  ) => {
    const peer = summary.peers?.[0];
    if (!peer) return;
    const plan = plans[folderId];
    const overwritesChangedSide =
      plan?.decision === "conflict" ||
      (plan?.decision === "pull" && action === "push") ||
      (plan?.decision === "push" && action === "pull");
    if (action !== "auto" && overwritesChangedSide && !confirmed) {
      setPending({ folderId, action });
      return;
    }
    setBusy(folderId);
    setPending(undefined);
    try {
      const result = await runSync(peer.id, folderId, action, confirmed);
      toaster.toast({
        title: "LAN Save Sync complete",
        body: `Version ${shortHash(result.root_hash)}`,
      });
      await refresh();
    } catch (error) {
      toaster.toast({
        title: "LAN Save Sync failed",
        body: String(error),
        critical: true,
      });
    } finally {
      setBusy(undefined);
    }
  };

  if (!summary.ready) {
    return (
      <PanelSection title="Agent unavailable">
        <PanelSectionRow>
          <div>{summary.error ?? "Loading configuration…"}</div>
        </PanelSectionRow>
        <PanelSectionRow>
          <ButtonItem onClick={() => void refresh()}>Retry</ButtonItem>
        </PanelSectionRow>
      </PanelSection>
    );
  }

  const peer = summary.peers?.[0];
  if (!peer) {
    return (
      <PanelSection title="Configuration required">
        <PanelSectionRow>
          Add at least one peer to ~/.config/lan-save-sync/agent.json.
        </PanelSectionRow>
      </PanelSection>
    );
  }

  return (
    <>
      <PanelSection title={`Peer: ${peer.name}`}>
        <PanelSectionRow>
          <ButtonItem onClick={() => void refresh()}>Refresh all</ButtonItem>
        </PanelSectionRow>
      </PanelSection>
      {(summary.folders ?? []).map((folder) => {
        const plan = plans[folder.id];
        const isPending = pending?.folderId === folder.id;
        return (
          <PanelSection key={folder.id} title={folder.name}>
            <PanelSectionRow>
              <div>
                {plan
                  ? `${plan.decision}: ${plan.reason}`
                  : "Checking versions…"}
              </div>
            </PanelSectionRow>
            {plan && (
              <PanelSectionRow>
                <div>
                  Deck {shortHash(plan.local_hash)} · Peer{" "}
                  {shortHash(plan.remote_hash)}
                </div>
              </PanelSectionRow>
            )}
            <PanelSectionRow>
              <ButtonItem
                disabled={busy === folder.id || !plan}
                onClick={() => void sync(folder.id, "auto")}
              >
                Safe sync
              </ButtonItem>
            </PanelSectionRow>
            <PanelSectionRow>
              <ButtonItem
                disabled={busy === folder.id || !plan}
                onClick={() => void sync(folder.id, "push")}
              >
                Push Deck → peer
              </ButtonItem>
            </PanelSectionRow>
            <PanelSectionRow>
              <ButtonItem
                disabled={busy === folder.id || !plan}
                onClick={() => void sync(folder.id, "pull")}
              >
                Pull peer → Deck
              </ButtonItem>
            </PanelSectionRow>
            {isPending && (
              <PanelSectionRow>
                <ButtonItem
                  onClick={() =>
                    void sync(folder.id, pending.action, true)
                  }
                >
                  Confirm overwrite ({pending.action})
                </ButtonItem>
              </PanelSectionRow>
            )}
          </PanelSection>
        );
      })}
    </>
  );
}

export default definePlugin(() => ({
  name: "LAN Save Sync",
  titleView: <div className={staticClasses.Title}>LAN Save Sync</div>,
  content: <Content />,
  icon: <FaArrowsRotate />,
  onDismount() {},
}));
