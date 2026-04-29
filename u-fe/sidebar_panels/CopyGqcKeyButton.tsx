// Copyright (c) Meta Platforms, Inc. and affiliates.

import { useState } from "react";
import { Check, Copy, Loader2 } from "lucide-react";
import type { ExplorerGraphSource } from "../Explorer";
import type { GraphQueryConfig } from "../__generated__/ts/GraphQueryConfig";
import UTooltip from "../components/UTooltip";
import { Button } from "../components/ui/button";
import { useRpc } from "../api/rpc";
import { useTVC } from "../context/TraversalConfigContext";

type HandleSource = ExplorerGraphSource & { type: "handle" };

export default function CopyGqcKeyButton({ source }: { source: HandleSource }) {
  const rpc = useRpc();
  const { tvcR } = useTVC();
  const [status, setStatus] = useState<"idle" | "loading" | "copied">("idle");
  const [copiedKey, setCopiedKey] = useState<string | null>(null);

  async function handleClick() {
    setStatus("loading");
    try {
      const gqc: GraphQueryConfig = {
        handle: source.right.handle,
        roots: source.right.roots,
        traversal: { Inline: tvcR },
      };
      const result = await rpc.call("PutConfigs", {
        traversal_configs: [],
        graph_query_configs: [gqc],
      });
      const key = result.graph_query_configs[0]!;
      await navigator.clipboard.writeText(key);
      setCopiedKey(key);
      setStatus(`copied`);
      setTimeout(() => setStatus("idle"), 2000);
    } catch (e) {
      const c = console;
      c.error("Failed to create GQC key:", e);
      setStatus("idle");
    }
  }

  return (
    <UTooltip
      tooltip={
        status === "copied" ? `Copied ${copiedKey}` : "Copy graph query key"
      }
    >
      <Button
        size="icon"
        className="cursor-pointer"
        variant="ghost"
        onClick={handleClick}
        disabled={status === "loading"}
      >
        {status === "loading" ? (
          <Loader2 className="animate-spin" />
        ) : status === "copied" ? (
          <Check className="text-green-500" />
        ) : (
          <Copy />
        )}
      </Button>
    </UTooltip>
  );
}
