// Copyright (c) Meta Platforms, Inc. and affiliates.
import { Download } from "lucide-react";
import { useState } from "react";
import type { ExportFormat } from "../__generated__/ts/ExportFormat";
import type { ExportScope } from "../__generated__/ts/ExportScope";
import { Button } from "../components/ui/button";
import { Card } from "../components/ui/card";
import { useNativeGraphs } from "../context/NativeGraphContext";
import { SidebarPanel, SidebarPanelHeader } from "./SidebarPanel";

// Everything client-side: WASM serializes the graph to bytes, we wrap them in a
// Blob and click a synthetic <a download>. No server round-trip, even at 100s of
// MB. Bytes (not a String) keep peak memory to a single copy.

const FORMATS: Record<
  ExportFormat,
  { label: string; extension: string; mime: string }
> = {
  MapGraphJson: { label: "JSON", extension: "json", mime: "application/json" },
  Gephi: {
    label: "Gephi (GEXF)",
    extension: "gexf",
    mime: "application/xml",
  },
};

const SCOPES: Record<ExportScope, { label: string; hint: string }> = {
  Reachable: {
    label: "Reachable",
    hint: "Only nodes reachable under the current traversal config. Excluded edges and unreachable nodes are dropped.",
  },
  Whole: {
    label: "Whole graph",
    hint: "Every node and edge, ignoring the traversal config.",
  },
};

export default function ExportGraphPanel() {
  const [nativeGraphL, nativeGraphR] = useNativeGraphs();
  const [format, setFormat] = useState<ExportFormat>("MapGraphJson");
  const [scope, setScope] = useState<ExportScope>("Reachable");
  const [error, setError] = useState<string | null>(null);
  const [exporting, setExporting] = useState(false);

  const isCompareMode = nativeGraphL != null;

  const onDownload = () => {
    setError(null);
    setExporting(true);
    try {
      const bytes = nativeGraphR.exportGraph(scope, format);
      const { extension, mime } = FORMATS[format];
      // wasm-bindgen returns a plain ArrayBuffer-backed Uint8Array (no threads),
      // so it's a valid BlobPart — the cast just sidesteps the SharedArrayBuffer
      // union in the DOM types. Blob makes its own copy; no pre-copy needed.
      const blob = new Blob([bytes as unknown as BlobPart], { type: mime });
      triggerDownload(blob, `graph.${extension}`);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setExporting(false);
    }
  };

  return (
    <SidebarPanel storageKey="export-graph">
      <SidebarPanelHeader text="Export Graph" />
      <div className="flex flex-col gap-6 pt-2">
        <Field label="Format">
          <Segmented
            options={Object.keys(FORMATS) as ExportFormat[]}
            value={format}
            onChange={setFormat}
            labelOf={(k) => FORMATS[k].label}
          />
        </Field>

        <Field label="Scope">
          <Segmented
            options={Object.keys(SCOPES) as ExportScope[]}
            value={scope}
            onChange={setScope}
            labelOf={(k) => SCOPES[k].label}
          />
          <p className="text-xs text-muted-foreground pt-2">
            {SCOPES[scope].hint}
          </p>
        </Field>

        {isCompareMode && (
          <Card className="p-3 text-xs text-muted-foreground">
            Comparison mode — exporting the right graph.
          </Card>
        )}

        <Button
          className="cursor-pointer"
          onClick={onDownload}
          disabled={exporting}
        >
          <Download />
          {exporting ? "Exporting…" : "Download"}
        </Button>

        {error != null && (
          <p className="text-xs text-destructive break-words">{error}</p>
        )}
      </div>
    </SidebarPanel>
  );
}

/// A row of mutually-exclusive buttons. The selected one uses the primary
/// (default) variant; the rest are outlined. All show a pointer cursor.
function Segmented<T extends string>({
  options,
  value,
  onChange,
  labelOf,
}: {
  options: T[];
  value: T;
  onChange: (value: T) => void;
  labelOf: (option: T) => string;
}) {
  return (
    <div className="flex gap-2">
      {options.map((option) => (
        <Button
          key={option}
          size="sm"
          variant={value === option ? "default" : "outline"}
          className="flex-1 cursor-pointer"
          onClick={() => onChange(option)}
        >
          {labelOf(option)}
        </Button>
      ))}
    </div>
  );
}

function Field({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <div className="flex flex-col gap-2">
      <div className="text-sm font-medium">{label}</div>
      {children}
    </div>
  );
}

function triggerDownload(blob: Blob, filename: string) {
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  a.remove();
  URL.revokeObjectURL(url);
}
