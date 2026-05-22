// Copyright (c) Meta Platforms, Inc. and affiliates.

import { Check, Copy, X } from "lucide-react";
import { useState } from "react";
import UTooltip from "./UTooltip";

type CopyState = "idle" | "success" | "error";

type Props = {
  text: string;
  size?: number;
  className?: string;
};

export default function CopyToClipboard({
  text,
  size = 14,
  className = "",
}: Props) {
  const [copyState, setCopyState] = useState<CopyState>("idle");

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(text);
      setCopyState("success");
      setTimeout(() => setCopyState("idle"), 3000);
    } catch (_err) {
      setCopyState("error");
      setTimeout(() => setCopyState("idle"), 3000);
    }
  };

  const getIcon = () => {
    switch (copyState) {
      case "success":
        return <Check size={size} className="text-green-600" />;
      case "error":
        return <X size={size} className="text-red-600" />;
      default:
        return <Copy size={size} />;
    }
  };

  const getTitle = () => {
    switch (copyState) {
      case "success":
        return "Copied!";
      case "error":
        return "Copy failed";
      default:
        return "Copy to clipboard";
    }
  };

  return (
    <UTooltip tooltip={getTitle()} delayDuration={500}>
      <button
        type="button"
        onClick={handleCopy}
        className={`cursor-pointer p-1 rounded hover:bg-gray-200 opacity-75 hover:opacity-100 transition-opacity ${className}`}
        title={getTitle()}
        disabled={copyState !== "idle"}
      >
        {getIcon()}
      </button>
    </UTooltip>
  );
}
